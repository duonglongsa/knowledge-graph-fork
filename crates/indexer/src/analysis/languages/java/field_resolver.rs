use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use parser_core::java::types::{JavaDefinitionInfo, JavaDefinitionMetadata, JavaDefinitionType};

/// Resolves Java field references to their actual values
///
/// This resolver handles ALL field types (not just static final constants):
/// - Static final constants: `public static final String API_URL = "..."`
/// - Static fields: `public static String baseUrl = "..."`
/// - Instance fields: `private String endpoint = "..."`
/// - Enum constants: `Environment.PRODUCTION`
///
/// # Resolution Strategy
/// 1. Parse reference to extract class and field name
/// 2. Look up field definition by FQN
/// 3. Extract initializer value (literal, placeholder, or expression)
/// 4. Return resolved value or None
///
/// # Features
/// - Thread-safe caching for performance
/// - Supports qualified references (e.g., `Config.URL`)
/// - Supports fully qualified references (e.g., `com.example.Config.URL`)
/// - Works with imports (e.g., `import com.example.Config; Config.URL`)
/// - Handles all access modifiers (public, private, protected)
/// - Works with static and instance fields
///
/// # Limitations
/// - Does not resolve runtime values (e.g., method calls)
/// - Does not follow field reassignments
/// - Only extracts literal values from initializers
///
/// # Example
/// ```java
/// public class ApiConfig {
///     public static final String BASE_URL = "http://api.example.com";
///     public static String version = "/v1";
///     private String endpoint = "/users";
/// }
///
/// // All these can be resolved:
/// ApiConfig.BASE_URL     → "http://api.example.com"
/// ApiConfig.version      → "/v1"
/// ApiConfig.endpoint     → "/users"
/// ```
pub struct FieldResolver {
    /// Cache of resolved fields: "ClassName.FIELD_NAME" -> "literal_value"
    cache: Arc<RwLock<HashMap<String, Option<String>>>>,
}

impl FieldResolver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect if a value looks like a field reference
    ///
    /// # Examples
    /// - `Config.BASE_URL` → true
    /// - `com.example.Config.API_PATH` → true
    /// - `BASE_URL` → true (could be a simple reference)
    /// - `"/api/v1"` → false (string literal)
    /// - `123` → false (number)
    pub fn is_field_reference(value: &str) -> bool {
        let trimmed = value.trim();

        // Skip obvious literals
        if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            return false;
        }
        if trimmed.parse::<i64>().is_ok() || trimmed.parse::<f64>().is_ok() {
            return false;
        }
        if trimmed == "true" || trimmed == "false" || trimmed == "null" {
            return false;
        }

        // Check if it looks like a field reference
        // Must contain at least one uppercase letter or a dot
        trimmed.contains('.') || trimmed.chars().any(|c| c.is_uppercase())
    }

    /// Attempt to resolve a field reference to its literal value
    ///
    /// # Arguments
    /// * `reference` - The field reference (e.g., "Config.BASE_URL" or "BASE_URL")
    /// * `definitions` - All Java definitions from the project (for lookup)
    /// * `context_package` - Optional package context for resolving simple names
    ///
    /// # Returns
    /// - `Some(String)` if field was resolved to a literal value
    /// - `None` if field could not be resolved
    pub fn resolve(
        &self,
        reference: &str,
        definitions: &[JavaDefinitionInfo],
        context_package: Option<&str>,
    ) -> Option<String> {
        let reference = reference.trim();

        // Quick check: is this even a field reference?
        if !Self::is_field_reference(reference) {
            return None;
        }

        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(reference) {
                return cached.clone();
            }
        }

        // Try to resolve the field
        let resolved_value = self.resolve_internal(reference, definitions, context_package);

        // Cache the result (even if None, to avoid repeated lookups)
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(reference.to_string(), resolved_value.clone());
        }

        resolved_value
    }

    /// Internal resolution logic
    fn resolve_internal(
        &self,
        reference: &str,
        definitions: &[JavaDefinitionInfo],
        context_package: Option<&str>,
    ) -> Option<String> {
        // Parse the reference to extract class name and field name
        let (class_name, field_name) = self.parse_reference(reference)?;

        log::debug!(
            "[FIELD_RESOLVER] Attempting to resolve: {} (class={:?}, field={}, package={:?})",
            reference,
            class_name,
            field_name,
            context_package
        );

        // Strategy 1: Try with fully qualified name
        if let Some(class_name) = &class_name {
            for definition in definitions {
                if let Some(found_value) = self.extract_field_value(
                    definition,
                    Some(class_name.as_str()),
                    &field_name,
                ) {
                    log::info!(
                        "[FIELD_RESOLVER] ✅ Resolved {} -> '{}'",
                        reference,
                        found_value
                    );
                    return Some(found_value);
                }
            }
        }

        // Strategy 2: Try with context package (for simple field names)
        if class_name.is_none() && context_package.is_some() {
            let package = context_package.unwrap();
            let qualified_reference = format!("{}.{}", package, field_name);

            for definition in definitions {
                if let Some(found_value) = self.extract_field_value(
                    definition,
                    None,
                    &qualified_reference,
                ) {
                    log::info!(
                        "[FIELD_RESOLVER] ✅ Resolved {} (via package {}) -> '{}'",
                        reference,
                        package,
                        found_value
                    );
                    return Some(found_value);
                }
            }
        }

        log::warn!(
            "[FIELD_RESOLVER] ❌ Could not resolve field reference: {}",
            reference
        );
        None
    }

    /// Parse field reference into class name and field name
    ///
    /// Examples:
    /// - "Config.BASE_URL" -> (Some("Config"), "BASE_URL")
    /// - "com.example.Config.API_PATH" -> (Some("com.example.Config"), "API_PATH")
    /// - "BASE_URL" -> (None, "BASE_URL")
    fn parse_reference(&self, reference: &str) -> Option<(Option<String>, String)> {
        let trimmed = reference.trim();

        if let Some(last_dot_idx) = trimmed.rfind('.') {
            let class_part = &trimmed[..last_dot_idx];
            let field_part = &trimmed[last_dot_idx + 1..];
            Some((Some(class_part.to_string()), field_part.to_string()))
        } else {
            // No dot means it's a simple field reference
            Some((None, trimmed.to_string()))
        }
    }

    /// Extract field value from a definition if it matches the search criteria
    fn extract_field_value(
        &self,
        definition: &JavaDefinitionInfo,
        target_class: Option<&str>,
        field_name: &String,
    ) -> Option<String> {
        // Only process Field definitions
        if definition.definition_type != JavaDefinitionType::Field {
            return None;
        }

        let def_fqn_parts = definition.fqn.as_ref();

        // Build FQN string for matching
        let fqn_string: String = def_fqn_parts
            .iter()
            .map(|part| part.node_name())
            .collect::<Vec<_>>()
            .join(".");

        // Check if field name matches (could be simple name or full FQN)
        let matches = if field_name.contains('.') {
            // field_name is a qualified name - check if FQN contains it
            fqn_string.contains(field_name.as_str()) || fqn_string.ends_with(field_name.as_str())
        } else {
            // field_name is a simple name - check last part
            let last_part = def_fqn_parts.last()?;
            last_part.node_name() == field_name.as_str()
        };

        if !matches {
            return None;
        }

        // If target_class is specified, verify the class name matches
        if let Some(target) = target_class {
            // Check if FQN contains the target class
            // This handles both "Config.BASE_URL" and "com.example.Config.BASE_URL"
            if !fqn_string.contains(target) {
                return None;
            }
        }

        // Extract the literal value from field metadata
        if let Some(JavaDefinitionMetadata::Field { initializer, modifiers, .. }) = &definition.metadata {
            // Build FQN string for logging
            let fqn_string: String = def_fqn_parts
                .iter()
                .map(|part| part.node_name())
                .collect::<Vec<_>>()
                .join(".");

            // Log field modifiers for debugging
            let is_static = modifiers.iter().any(|m| m == "static");
            let is_final = modifiers.iter().any(|m| m == "final");
            let access = modifiers.iter()
                .find(|m| matches!(m.as_str(), "public" | "private" | "protected"))
                .map(|m| m.as_str())
                .unwrap_or("default");

            log::debug!(
                "[FIELD_RESOLVER] Found field {} (static={}, final={}, access={})",
                fqn_string,
                is_static,
                is_final,
                access
            );

            // Extract literal value from initializer (works for all field types)
            // REMOVED: The static/final check that was in ConstantResolver
            if let Some(init_value) = initializer {
                return self.extract_literal_from_initializer(init_value, definition, &fqn_string);
            } else {
                log::debug!(
                    "[FIELD_RESOLVER] Field {} has no initializer",
                    fqn_string
                );
            }
        }

        None
    }

    /// Extract literal value from field initializer
    ///
    /// Supports:
    /// - String literals: `"value"` -> `value`
    /// - Numeric literals: `123` -> `123`
    /// - Boolean literals: `true` -> `true`
    ///
    /// Does not support (Phase 1):
    /// - Concatenation: `BASE + "/path"`
    /// - Method calls: `String.format(...)`
    /// - Object creation: `new URL(...)`
    fn extract_literal_from_initializer(
        &self,
        initializer: &str,
        _definition: &JavaDefinitionInfo,
        fqn_string: &str,
    ) -> Option<String> {
        let trimmed = initializer.trim();

        // Extract string literal
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            let literal = &trimmed[1..trimmed.len() - 1];
            log::debug!(
                "[FIELD_RESOLVER] Extracted string literal '{}' from {}",
                literal,
                fqn_string
            );
            return Some(literal.to_string());
        }

        // Extract numeric or boolean literal
        if trimmed.parse::<i64>().is_ok()
            || trimmed.parse::<f64>().is_ok()
            || trimmed == "true"
            || trimmed == "false"
        {
            log::debug!(
                "[FIELD_RESOLVER] Extracted primitive literal '{}' from {}",
                trimmed,
                fqn_string
            );
            return Some(trimmed.to_string());
        }

        // TODO Phase 2: Handle field references (recursive resolution)
        // if Self::is_field_reference(trimmed) {
        //     return self.resolve(trimmed, definitions, None);
        // }

        // TODO Phase 2: Handle simple concatenation
        // if trimmed.contains('+') { ... }

        log::debug!(
            "[FIELD_RESOLVER] Cannot extract literal from initializer '{}' for {}",
            trimmed,
            fqn_string
        );
        None
    }

    /// Clear the cache (useful for testing or when index is rebuilt)
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }
}

impl Default for FieldResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser_core::java::types::{JavaFqnPart, JavaFqnPartType, JavaType};
    use parser_core::utils::{Range, Position};
    use smallvec::SmallVec;
    use std::sync::Arc;

    fn create_field_definition(
        fqn_str: &str,
        field_name: &str,
        initializer: Option<&str>,
        modifiers: Vec<String>,
    ) -> JavaDefinitionInfo {
        let fqn_parts: Vec<&str> = fqn_str.split('.').collect();
        let mut fqn: SmallVec<[JavaFqnPart; 8]> = SmallVec::new();

        let empty_range = Range {
            start: Position { line: 0, column: 0 },
            end: Position { line: 0, column: 0 },
            byte_offset: (0, 0),
        };

        for (i, part) in fqn_parts.iter().enumerate() {
            let part_type = if i == fqn_parts.len() - 1 {
                JavaFqnPartType::Field
            } else {
                JavaFqnPartType::Class
            };
            fqn.push(JavaFqnPart::new(
                part_type,
                part.to_string(),
                empty_range.clone(),
            ));
        }

        let mut definition = JavaDefinitionInfo::new(
            JavaDefinitionType::Field,
            field_name.to_string(),
            Arc::new(fqn),
            empty_range,
        );

        definition.metadata = Some(JavaDefinitionMetadata::Field {
            initializer: initializer.map(|s| s.to_string()),
            modifiers,
            annotations: vec![],
            field_type: JavaType { name: "String".to_string() },
        });

        definition
    }

    #[test]
    fn test_is_field_reference() {
        // Should detect field references
        assert!(FieldResolver::is_field_reference("Config.BASE_URL"));
        assert!(FieldResolver::is_field_reference("com.example.Config.API_PATH"));
        assert!(FieldResolver::is_field_reference("BASE_URL"));
        assert!(FieldResolver::is_field_reference("ApiConfig.Version.V1"));

        // Should not detect literals
        assert!(!FieldResolver::is_field_reference("\"/api/v1\""));
        assert!(!FieldResolver::is_field_reference("123"));
        assert!(!FieldResolver::is_field_reference("123.45"));
        assert!(!FieldResolver::is_field_reference("true"));
        assert!(!FieldResolver::is_field_reference("false"));
        assert!(!FieldResolver::is_field_reference("null"));
    }

    #[test]
    fn test_resolve_static_final_constant() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.Config.BASE_URL",
                "BASE_URL",
                Some("\"http://api.example.com\""),
                vec!["public".to_string(), "static".to_string(), "final".to_string()],
            ),
        ];

        assert_eq!(
            resolver.resolve("Config.BASE_URL", &definitions, None),
            Some("http://api.example.com".to_string())
        );
    }

    #[test]
    fn test_resolve_static_field() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.Config.version",
                "version",
                Some("\"/v1\""),
                vec!["public".to_string(), "static".to_string()],
            ),
        ];

        assert_eq!(
            resolver.resolve("Config.version", &definitions, None),
            Some("/v1".to_string())
        );
    }

    #[test]
    fn test_resolve_instance_field() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.Config.endpoint",
                "endpoint",
                Some("\"/users\""),
                vec!["private".to_string()],
            ),
        ];

        assert_eq!(
            resolver.resolve("Config.endpoint", &definitions, None),
            Some("/users".to_string())
        );
    }

    #[test]
    fn test_resolve_with_package_context() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.BASE_PATH",
                "BASE_PATH",
                Some("\"/api\""),
                vec!["public".to_string(), "static".to_string()],
            ),
        ];

        // Should resolve simple name with package context
        assert_eq!(
            resolver.resolve("BASE_PATH", &definitions, Some("com.example")),
            Some("/api".to_string())
        );
    }

    #[test]
    fn test_cache_works() {
        let resolver = FieldResolver::new();
        let definitions = vec![];

        // First call - miss
        let result1 = resolver.resolve("Config.UNKNOWN", &definitions, None);
        assert_eq!(result1, None);

        // Second call - should hit cache
        let result2 = resolver.resolve("Config.UNKNOWN", &definitions, None);
        assert_eq!(result2, None);

        // Verify cache contains the entry
        let cache = resolver.cache.read().unwrap();
        assert!(cache.contains_key("Config.UNKNOWN"));
    }

    #[test]
    fn test_extract_numeric_literal() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.Config.PORT",
                "PORT",
                Some("8080"),
                vec!["public".to_string(), "static".to_string(), "final".to_string()],
            ),
        ];

        assert_eq!(
            resolver.resolve("Config.PORT", &definitions, None),
            Some("8080".to_string())
        );
    }

    #[test]
    fn test_extract_boolean_literal() {
        let resolver = FieldResolver::new();
        let definitions = vec![
            create_field_definition(
                "com.example.Config.ENABLED",
                "ENABLED",
                Some("true"),
                vec!["public".to_string(), "static".to_string(), "final".to_string()],
            ),
        ];

        assert_eq!(
            resolver.resolve("Config.ENABLED", &definitions, None),
            Some("true".to_string())
        );
    }

    #[test]
    fn test_parse_reference_qualified() {
        let resolver = FieldResolver::new();

        let (class, field) = resolver.parse_reference("Config.BASE_URL").unwrap();
        assert_eq!(class, Some("Config".to_string()));
        assert_eq!(field, "BASE_URL");
    }

    #[test]
    fn test_parse_reference_fully_qualified() {
        let resolver = FieldResolver::new();

        let (class, field) = resolver.parse_reference("com.example.Config.API_PATH").unwrap();
        assert_eq!(class, Some("com.example.Config".to_string()));
        assert_eq!(field, "API_PATH");
    }

    #[test]
    fn test_parse_reference_simple() {
        let resolver = FieldResolver::new();

        let (class, field) = resolver.parse_reference("BASE_URL").unwrap();
        assert_eq!(class, None);
        assert_eq!(field, "BASE_URL");
    }
}
