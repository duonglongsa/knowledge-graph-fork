use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use parser_core::java::types::{JavaDefinitionInfo, JavaDefinitionMetadata, JavaDefinitionType};

/// Resolves Java constant references to their actual values
///
/// When annotations use constant references like `@RequestMapping(Constant.PATH)`,
/// this resolver finds the constant definition and extracts its literal value.
///
/// # Features
/// - Thread-safe caching for performance
/// - Supports qualified references (e.g., `Constant.PATH`)
/// - Supports fully qualified references (e.g., `com.example.Constant.PATH`)
/// - Handles static final fields with string literals
///
/// # Limitations (Phase 1 - MVP)
/// - Only resolves string literals (e.g., `"value"`)
/// - Does not support string concatenation (e.g., `BASE + "/path"`)
/// - Does not support method calls (e.g., `String.format(...)`)
/// - Does not resolve runtime values (e.g., `System.getenv(...)`)
///
/// # Example
/// ```java
/// public class Constant {
///     public static final String PATH = "/api/v1";
/// }
///
/// @RequestMapping(Constant.PATH)  // Resolves to "/api/v1"
/// ```
pub struct ConstantResolver {
    /// Cache of resolved constants: "ClassName.FIELD_NAME" -> "literal_value"
    cache: Arc<RwLock<HashMap<String, Option<String>>>>,
}

impl ConstantResolver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect if a value looks like a constant reference
    ///
    /// # Examples
    /// - `Constant.PATH` -> true
    /// - `com.example.Constant.PATH` -> true
    /// - `PATH` -> true (could be a simple reference)
    /// - `"/api/v1"` -> false (string literal)
    /// - `123` -> false (number)
    pub fn is_constant_reference(value: &str) -> bool {
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

        // Check if it looks like a constant reference
        // Must contain at least one uppercase letter or a dot
        trimmed.contains('.') || trimmed.chars().any(|c| c.is_uppercase())
    }

    /// Attempt to resolve a constant reference to its literal value
    ///
    /// # Arguments
    /// * `reference` - The constant reference (e.g., "Constant.PATH" or "PATH")
    /// * `definitions` - All Java definitions from the project (for lookup)
    ///
    /// # Returns
    /// - `Some(String)` if constant was resolved to a literal value
    /// - `None` if constant could not be resolved
    pub fn resolve(
        &self,
        reference: &str,
        definitions: &[JavaDefinitionInfo],
    ) -> Option<String> {
        let reference = reference.trim();

        // Quick check: is this even a constant reference?
        if !Self::is_constant_reference(reference) {
            return None;
        }

        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(cached) = cache.get(reference) {
                return cached.clone();
            }
        }

        // Try to resolve the constant
        let resolved_value = self.resolve_internal(reference, definitions);

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
    ) -> Option<String> {
        // Parse the reference to extract class name and field name
        let (class_name, field_name) = self.parse_reference(reference)?;

        log::debug!(
            "[CONSTANT_RESOLVER] Attempting to resolve: {} (class={:?}, field={})",
            reference,
            class_name,
            field_name
        );

        // Find the field definition
        for definition in definitions {
            if let Some(found_value) = self.extract_field_value(
                definition,
                class_name.as_deref(),
                &field_name,
            ) {
                log::info!(
                    "[CONSTANT_RESOLVER] ✅ Resolved {} -> '{}'",
                    reference,
                    found_value
                );
                return Some(found_value);
            }
        }

        log::warn!(
            "[CONSTANT_RESOLVER] ❌ Could not resolve constant reference: {}",
            reference
        );
        None
    }

    /// Parse constant reference into class name and field name
    ///
    /// Examples:
    /// - "Constant.PATH" -> (Some("Constant"), "PATH")
    /// - "com.example.Constant.PATH" -> (Some("com.example.Constant"), "PATH")
    /// - "PATH" -> (None, "PATH")
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

        // Check if field name matches
        let def_fqn_parts = definition.fqn.as_ref();
        let last_part = def_fqn_parts.last()?;
        if last_part.node_name() != field_name.as_str() {
            return None;
        }

        // If target_class is specified, verify the class name matches
        if let Some(target) = target_class {
            // Build FQN string manually
            let fqn_string: String = def_fqn_parts
                .iter()
                .map(|part| part.node_name())
                .collect::<Vec<_>>()
                .join(".");

            // Check if FQN contains the target class
            // This handles both "Constant.PATH" and "com.example.Constant.PATH"
            if !fqn_string.contains(target) {
                return None;
            }
        }

        // Extract the literal value from field metadata
        if let Some(JavaDefinitionMetadata::Field { initializer, modifiers, .. }) = &definition.metadata {
            // Check if it's a static final field (constant)
            let is_static = modifiers.iter().any(|m| m == "static");
            let is_final = modifiers.iter().any(|m| m == "final");

            // Build FQN string for logging
            let fqn_string: String = def_fqn_parts
                .iter()
                .map(|part| part.node_name())
                .collect::<Vec<_>>()
                .join(".");

            if !is_static || !is_final {
                log::debug!(
                    "[CONSTANT_RESOLVER] Field {} is not static final (static={}, final={})",
                    fqn_string,
                    is_static,
                    is_final
                );
                return None;
            }

            // Extract literal value from initializer
            if let Some(init_value) = initializer {
                return self.extract_literal_from_initializer(init_value, definition, &fqn_string);
            } else {
                log::debug!(
                    "[CONSTANT_RESOLVER] Field {} has no initializer",
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
    /// - Simple references: `OtherConstant.VALUE` -> (needs recursive resolution, Phase 2)
    ///
    /// Does not support (Phase 1):
    /// - Concatenation: `BASE + "/path"`
    /// - Method calls: `String.format(...)`
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
                "[CONSTANT_RESOLVER] Extracted string literal '{}' from {}",
                literal,
                fqn_string
            );
            return Some(literal.to_string());
        }

        // TODO Phase 2: Handle constant references (recursive resolution)
        // if Self::is_constant_reference(trimmed) {
        //     return self.resolve(trimmed, definitions);
        // }

        // TODO Phase 2: Handle simple concatenation
        // if trimmed.contains('+') { ... }

        log::debug!(
            "[CONSTANT_RESOLVER] Cannot extract literal from initializer '{}' for {}",
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

impl Default for ConstantResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_constant_reference() {
        // Should detect constant references
        assert!(ConstantResolver::is_constant_reference("Constant.PATH"));
        assert!(ConstantResolver::is_constant_reference("com.example.Constant.PATH"));
        assert!(ConstantResolver::is_constant_reference("PATH"));
        assert!(ConstantResolver::is_constant_reference("Config.Api.PATH"));

        // Should not detect literals
        assert!(!ConstantResolver::is_constant_reference("\"/api/v1\""));
        assert!(!ConstantResolver::is_constant_reference("123"));
        assert!(!ConstantResolver::is_constant_reference("123.45"));
        assert!(!ConstantResolver::is_constant_reference("true"));
        assert!(!ConstantResolver::is_constant_reference("false"));
        assert!(!ConstantResolver::is_constant_reference("null"));
    }

    #[test]
    fn test_parse_reference_qualified() {
        let resolver = ConstantResolver::new();

        let (class, field) = resolver.parse_reference("Constant.PATH").unwrap();
        assert_eq!(class, Some("Constant".to_string()));
        assert_eq!(field, "PATH");
    }

    #[test]
    fn test_parse_reference_fully_qualified() {
        let resolver = ConstantResolver::new();

        let (class, field) = resolver.parse_reference("com.example.Constant.PATH").unwrap();
        assert_eq!(class, Some("com.example.Constant".to_string()));
        assert_eq!(field, "PATH");
    }

    #[test]
    fn test_parse_reference_simple() {
        let resolver = ConstantResolver::new();

        let (class, field) = resolver.parse_reference("PATH").unwrap();
        assert_eq!(class, None);
        assert_eq!(field, "PATH");
    }

    #[test]
    fn test_extract_literal_from_initializer() {
        let resolver = ConstantResolver::new();

        // Mock definition (only FQN string is used in logging)
        use std::sync::Arc;
        use smallvec::SmallVec;
        use parser_core::java::types::{JavaFqnPart, JavaFqnPartType};
        use parser_core::range::Range;

        let fqn_part = JavaFqnPart::new(
            JavaFqnPartType::Field,
            "TEST".to_string(),
            Range { start_byte: 0, end_byte: 0, start_line: 0, end_line: 0 },
        );
        let fqn: SmallVec<[JavaFqnPart; 8]> = SmallVec::from_vec(vec![fqn_part]);
        let definition = JavaDefinitionInfo::new(
            JavaDefinitionType::Field,
            "TEST".to_string(),
            Arc::new(fqn),
            Range { start_byte: 0, end_byte: 0, start_line: 0, end_line: 0 },
        );

        // Test string literal extraction
        assert_eq!(
            resolver.extract_literal_from_initializer("\"/api/v1\"", &definition, "TEST"),
            Some("/api/v1".to_string())
        );

        assert_eq!(
            resolver.extract_literal_from_initializer("\"hello world\"", &definition, "TEST"),
            Some("hello world".to_string())
        );

        // Test non-supported cases
        assert_eq!(
            resolver.extract_literal_from_initializer("BASE + \"/path\"", &definition, "TEST"),
            None
        );

        assert_eq!(
            resolver.extract_literal_from_initializer("String.format(\"/api/%s\", version)", &definition, "TEST"),
            None
        );
    }

    #[test]
    fn test_cache_works() {
        let resolver = ConstantResolver::new();
        let definitions = vec![];

        // First call - miss
        let result1 = resolver.resolve("Constant.PATH", &definitions);
        assert_eq!(result1, None);

        // Second call - should hit cache
        let result2 = resolver.resolve("Constant.PATH", &definitions);
        assert_eq!(result2, None);

        // Verify cache contains the entry
        let cache = resolver.cache.read().unwrap();
        assert!(cache.contains_key("Constant.PATH"));
    }
}
