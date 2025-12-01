use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Resolves Spring property placeholders like ${property.key}
///
/// This resolver handles Spring Boot property placeholder syntax commonly used
/// in Java applications for externalizing configuration values.
///
/// # Features
/// - Thread-safe caching for performance
/// - Supports default values: `${key:defaultValue}`
/// - Handles multiple placeholders in a single string
/// - Preserves original placeholders if not found
///
/// # Placeholder Syntax
/// - Basic: `${property.key}` → looks up "property.key" in context
/// - With default: `${property.key:default}` → uses "default" if not found
/// - Multiple: `${protocol}://${host}:${port}` → resolves all placeholders
///
/// # Limitations (Phase 1 - MVP)
/// - Does not load from application.properties/yml files (manual `add_property` only)
/// - Does not support nested placeholders: `${prefix.${env}.suffix}`
/// - Does not support SpEL expressions: `${#{systemProperties['key']}}`
/// - Does not handle profile-specific properties
///
/// # Example
/// ```rust
/// let resolver = PropertyResolver::new();
/// resolver.add_property("api.host".to_string(), "http://localhost:8080".to_string());
/// resolver.add_property("api.version".to_string(), "v1".to_string());
///
/// // Basic resolution
/// assert_eq!(
///     resolver.resolve("${api.host}/users"),
///     "http://localhost:8080/users"
/// );
///
/// // Multiple placeholders
/// assert_eq!(
///     resolver.resolve("${api.host}/${api.version}/users"),
///     "http://localhost:8080/v1/users"
/// );
///
/// // With default value
/// assert_eq!(
///     resolver.resolve("${unknown:defaultValue}"),
///     "defaultValue"
/// );
///
/// // Missing property (no default) - keeps original
/// assert_eq!(
///     resolver.resolve("${unknown.property}"),
///     "${unknown.property}"
/// );
/// ```
pub struct PropertyResolver {
    /// Property context: "property.key" -> "resolved_value"
    context: Arc<RwLock<HashMap<String, String>>>,
}

impl PropertyResolver {
    pub fn new() -> Self {
        Self {
            context: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect if value contains Spring property placeholder
    ///
    /// # Examples
    /// - `"${api.host}"` → true
    /// - `"http://${host}:${port}"` → true
    /// - `"http://localhost"` → false
    /// - `"$host"` → false (missing braces)
    pub fn has_placeholder(value: &str) -> bool {
        value.contains("${") && value.contains('}')
    }

    /// Resolve all placeholders in a string
    ///
    /// # Algorithm
    /// 1. Find `${` starting position
    /// 2. Find matching `}` closing position
    /// 3. Extract placeholder key (and optional default value)
    /// 4. Lookup in context or use default
    /// 5. Replace placeholder with resolved value
    /// 6. Repeat until no more placeholders
    ///
    /// # Arguments
    /// * `value` - The string containing placeholders
    ///
    /// # Returns
    /// String with all placeholders resolved (or original placeholders if not found)
    ///
    /// # Examples
    /// ```
    /// // Single placeholder
    /// "${api.host}/path" → "http://localhost:8080/path"
    ///
    /// // Multiple placeholders
    /// "${protocol}://${host}:${port}" → "https://api.com:443"
    ///
    /// // With default value
    /// "${unknown:default}" → "default"
    ///
    /// // Missing (no default)
    /// "${missing}" → "${missing}"
    /// ```
    pub fn resolve(&self, value: &str) -> String {
        // Early exit if no placeholders
        if !Self::has_placeholder(value) {
            return value.to_string();
        }

        let mut result = value.to_string();
        let context = self.context.read().unwrap();

        // Track position to avoid infinite loops when placeholder is not resolved
        let mut search_start = 0;

        // Loop to handle multiple placeholders
        while let Some(relative_start) = result[search_start..].find("${") {
            let start = search_start + relative_start;

            if let Some(end_offset) = result[start..].find('}') {
                let end = start + end_offset;

                // Extract placeholder content (between ${ and })
                let placeholder_content = result[start + 2..end].to_string();

                // Parse default value syntax: ${key:default}
                let (key, default) = if let Some(colon_pos) = placeholder_content.find(':') {
                    (placeholder_content[..colon_pos].to_string(), Some(placeholder_content[colon_pos + 1..].to_string()))
                } else {
                    (placeholder_content.clone(), None)
                };

                // Resolve from context
                let resolved_value = if let Some(value) = context.get(&key) {
                    value.to_string()
                } else if let Some(def) = default {
                    def
                } else {
                    // Not resolved - skip this placeholder and continue
                    log::debug!(
                        "[PROPERTY_RESOLVER] Could not resolve ${{{}}}, keeping original",
                        key
                    );
                    search_start = end + 1; // Move past this placeholder
                    continue;
                };

                log::debug!(
                    "[PROPERTY_RESOLVER] Resolved ${{{key}}} -> '{resolved_value}'"
                );

                // Replace placeholder in string
                result.replace_range(start..=end, &resolved_value);

                // Continue searching from where we replaced
                search_start = start + resolved_value.len();
            } else {
                // Malformed placeholder (missing closing brace)
                log::warn!(
                    "[PROPERTY_RESOLVER] Malformed placeholder (missing '}}') in: {}",
                    &result[start..]
                );
                break;
            }
        }

        result
    }

    /// Manually add property to context (for testing or manual configuration)
    ///
    /// # Arguments
    /// * `key` - Property key (e.g., "api.host")
    /// * `value` - Property value (e.g., "http://localhost:8080")
    pub fn add_property(&self, key: String, value: String) {
        let mut context = self.context.write().unwrap();
        context.insert(key, value);
    }

    /// Load properties from a file
    ///
    /// Supports multiple file formats:
    /// - `.properties` - Java properties format (key=value)
    /// - `.yml`, `.yaml` - YAML format (nested structure converted to dot notation)
    /// - `.json` - JSON format
    ///
    /// # Arguments
    /// * `file_path` - Path to the property file
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of properties loaded
    /// * `Err(io::Error)` - If file cannot be read or format is unsupported
    ///
    /// # Examples
    /// ```rust
    /// let resolver = PropertyResolver::new();
    /// resolver.load_from_file("config/application.properties")?;
    /// ```
    pub fn load_from_file(&self, file_path: &str) -> std::io::Result<usize> {
        let path = Path::new(file_path);

        // Auto-detect format from extension
        match path.extension().and_then(|s| s.to_str()) {
            Some("properties") => self.load_properties_format(file_path),
            Some("yml") | Some("yaml") => self.load_yaml_format(file_path),
            Some("json") => self.load_json_format(file_path),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unsupported file format: {}", file_path),
            )),
        }
    }

    /// Load multiple files in order (later files override earlier)
    ///
    /// This is useful for loading base configuration followed by environment-specific
    /// overrides (e.g., application.properties then application-prod.properties).
    ///
    /// # Arguments
    /// * `file_paths` - Ordered list of property file paths
    ///
    /// # Returns
    /// * `Ok(usize)` - Total number of properties loaded (after overrides)
    /// * `Err(io::Error)` - If any file cannot be read
    ///
    /// # Examples
    /// ```rust
    /// let resolver = PropertyResolver::new();
    /// resolver.load_from_files(&[
    ///     "config/application.properties".to_string(),
    ///     "config/application-prod.properties".to_string(),
    /// ])?;
    /// ```
    pub fn load_from_files(&self, file_paths: &[String]) -> std::io::Result<usize> {
        let mut total_count = 0;
        for path in file_paths {
            total_count += self.load_from_file(path)?;
        }

        // Return total unique properties (accounting for overrides)
        let context = self.context.read().unwrap();
        Ok(context.len())
    }

    /// Load properties from .properties file format
    ///
    /// Supports:
    /// - Comments starting with `#` or `!`
    /// - Multi-line values with backslash continuation
    /// - Key-value pairs with `=` or `:` separator
    ///
    /// # Format Examples
    /// ```properties
    /// # Comment
    /// api.host=http://localhost:8080
    /// api.version : v1
    /// api.description=This is a very long \
    ///                description that spans \
    ///                multiple lines
    /// ```
    fn load_properties_format(&self, file_path: &str) -> std::io::Result<usize> {
        let content = std::fs::read_to_string(file_path)?;
        let mut context = self.context.write().unwrap();
        let mut count = 0;

        let mut current_line = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                continue;
            }

            // Handle multi-line values (ending with backslash)
            if trimmed.ends_with('\\') {
                current_line.push_str(&trimmed[..trimmed.len() - 1]);
                continue;
            }

            // Add current line
            current_line.push_str(trimmed);

            // Parse key=value or key:value
            if let Some(separator_pos) = current_line.find('=').or_else(|| current_line.find(':'))
            {
                let key = current_line[..separator_pos].trim();
                let value = current_line[separator_pos + 1..].trim();

                if !key.is_empty() {
                    context.insert(key.to_string(), value.to_string());
                    count += 1;
                }
            }

            // Reset for next property
            current_line.clear();
        }

        Ok(count)
    }

    /// Load properties from YAML file format
    ///
    /// Converts nested YAML structure to dot notation.
    ///
    /// # Format Examples
    /// ```yaml
    /// api:
    ///   host: http://localhost:8080
    ///   version: v1
    ///   timeout: 5000
    /// ```
    ///
    /// Becomes:
    /// - `api.host` → `http://localhost:8080`
    /// - `api.version` → `v1`
    /// - `api.timeout` → `5000`
    fn load_yaml_format(&self, file_path: &str) -> std::io::Result<usize> {
        let content = std::fs::read_to_string(file_path)?;

        // Parse YAML using serde_yaml
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("YAML parse error: {}", e))
        })?;

        let mut context = self.context.write().unwrap();
        let mut count = 0;

        // Flatten YAML structure into dot notation
        self.flatten_yaml(&yaml, String::new(), &mut context, &mut count);

        Ok(count)
    }

    /// Recursively flatten YAML structure into dot notation
    fn flatten_yaml(
        &self,
        value: &serde_yaml::Value,
        prefix: String,
        context: &mut HashMap<String, String>,
        count: &mut usize,
    ) {
        match value {
            serde_yaml::Value::Mapping(map) => {
                for (key, val) in map {
                    if let Some(key_str) = key.as_str() {
                        let new_prefix = if prefix.is_empty() {
                            key_str.to_string()
                        } else {
                            format!("{}.{}", prefix, key_str)
                        };
                        self.flatten_yaml(val, new_prefix, context, count);
                    }
                }
            }
            serde_yaml::Value::String(s) => {
                context.insert(prefix, s.clone());
                *count += 1;
            }
            serde_yaml::Value::Number(n) => {
                context.insert(prefix, n.to_string());
                *count += 1;
            }
            serde_yaml::Value::Bool(b) => {
                context.insert(prefix, b.to_string());
                *count += 1;
            }
            serde_yaml::Value::Null => {
                context.insert(prefix, String::new());
                *count += 1;
            }
            serde_yaml::Value::Sequence(_) => {
                // Skip arrays for now (could be extended to support indexed access)
                log::debug!("Skipping array property: {}", prefix);
            }
            serde_yaml::Value::Tagged(_) => {
                log::debug!("Skipping tagged property: {}", prefix);
            }
        }
    }

    /// Load properties from JSON file format
    ///
    /// Supports flat and nested JSON structures.
    ///
    /// # Format Examples
    /// ```json
    /// {
    ///   "api.host": "http://localhost:8080",
    ///   "api": {
    ///     "version": "v1"
    ///   }
    /// }
    /// ```
    ///
    /// Both flat keys and nested structures are supported.
    fn load_json_format(&self, file_path: &str) -> std::io::Result<usize> {
        let content = std::fs::read_to_string(file_path)?;

        // Parse JSON using serde_json
        let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("JSON parse error: {}", e))
        })?;

        let mut context = self.context.write().unwrap();
        let mut count = 0;

        // Flatten JSON structure into dot notation
        self.flatten_json(&json, String::new(), &mut context, &mut count);

        Ok(count)
    }

    /// Recursively flatten JSON structure into dot notation
    fn flatten_json(
        &self,
        value: &serde_json::Value,
        prefix: String,
        context: &mut HashMap<String, String>,
        count: &mut usize,
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    self.flatten_json(val, new_prefix, context, count);
                }
            }
            serde_json::Value::String(s) => {
                context.insert(prefix, s.clone());
                *count += 1;
            }
            serde_json::Value::Number(n) => {
                context.insert(prefix, n.to_string());
                *count += 1;
            }
            serde_json::Value::Bool(b) => {
                context.insert(prefix, b.to_string());
                *count += 1;
            }
            serde_json::Value::Null => {
                context.insert(prefix, String::new());
                *count += 1;
            }
            serde_json::Value::Array(_) => {
                // Skip arrays for now
                log::debug!("Skipping array property: {}", prefix);
            }
        }
    }

    /// Clear the cache (useful for testing or when reindexing)
    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        let mut context = self.context.write().unwrap();
        context.clear();
    }
}

impl Default for PropertyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_placeholder() {
        // Should detect placeholders
        assert!(PropertyResolver::has_placeholder("${api.host}"));
        assert!(PropertyResolver::has_placeholder("http://${host}:${port}"));
        assert!(PropertyResolver::has_placeholder("prefix_${value}_suffix"));

        // Should not detect non-placeholders
        assert!(!PropertyResolver::has_placeholder("http://localhost"));
        assert!(!PropertyResolver::has_placeholder("$host"));
        assert!(!PropertyResolver::has_placeholder("{value}"));
        assert!(!PropertyResolver::has_placeholder("${"));
        assert!(!PropertyResolver::has_placeholder("}"));
    }

    #[test]
    fn test_single_placeholder() {
        let resolver = PropertyResolver::new();
        resolver.add_property("host".to_string(), "localhost".to_string());

        assert_eq!(
            resolver.resolve("http://${host}/api"),
            "http://localhost/api"
        );
    }

    #[test]
    fn test_multiple_placeholders() {
        let resolver = PropertyResolver::new();
        resolver.add_property("protocol".to_string(), "https".to_string());
        resolver.add_property("host".to_string(), "api.example.com".to_string());
        resolver.add_property("port".to_string(), "443".to_string());

        assert_eq!(
            resolver.resolve("${protocol}://${host}:${port}/v1"),
            "https://api.example.com:443/v1"
        );
    }

    #[test]
    fn test_default_value() {
        let resolver = PropertyResolver::new();

        // Missing property with default
        assert_eq!(
            resolver.resolve("${missing:default}/path"),
            "default/path"
        );

        // Empty default
        assert_eq!(
            resolver.resolve("${missing:}"),
            ""
        );
    }

    #[test]
    fn test_missing_property_no_default() {
        let resolver = PropertyResolver::new();

        // Keep original placeholder
        assert_eq!(
            resolver.resolve("${missing}/path"),
            "${missing}/path"
        );
    }

    #[test]
    fn test_malformed_placeholder() {
        let resolver = PropertyResolver::new();

        // Missing closing brace
        assert_eq!(
            resolver.resolve("${api.host/path"),
            "${api.host/path"
        );

        // Missing opening brace
        assert_eq!(
            resolver.resolve("api.host}/path"),
            "api.host}/path"
        );
    }

    #[test]
    fn test_no_placeholder() {
        let resolver = PropertyResolver::new();

        // Plain string - no modification
        assert_eq!(
            resolver.resolve("http://localhost:8080/api"),
            "http://localhost:8080/api"
        );
    }

    #[test]
    fn test_placeholder_with_dots() {
        let resolver = PropertyResolver::new();
        resolver.add_property("payment.hub.host".to_string(), "http://payment-service:8080".to_string());
        resolver.add_property("payment.hub.api.get_token".to_string(), "api/v1/auth/token".to_string());

        assert_eq!(
            resolver.resolve("${payment.hub.host}/${payment.hub.api.get_token}"),
            "http://payment-service:8080/api/v1/auth/token"
        );
    }

    #[test]
    fn test_default_value_with_special_chars() {
        let resolver = PropertyResolver::new();

        // Default with slash
        assert_eq!(
            resolver.resolve("${missing:/api/v1}"),
            "/api/v1"
        );

        // Default with URL
        assert_eq!(
            resolver.resolve("${missing:http://localhost:8080}"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn test_consecutive_placeholders() {
        let resolver = PropertyResolver::new();
        resolver.add_property("protocol".to_string(), "https".to_string());
        resolver.add_property("host".to_string(), "api.com".to_string());

        assert_eq!(
            resolver.resolve("${protocol}://${host}"),
            "https://api.com"
        );
    }

    #[test]
    fn test_placeholder_at_boundaries() {
        let resolver = PropertyResolver::new();
        resolver.add_property("value".to_string(), "test".to_string());

        // Placeholder at start
        assert_eq!(
            resolver.resolve("${value}/path"),
            "test/path"
        );

        // Placeholder at end
        assert_eq!(
            resolver.resolve("/path/${value}"),
            "/path/test"
        );

        // Placeholder only
        assert_eq!(
            resolver.resolve("${value}"),
            "test"
        );
    }

    #[test]
    fn test_clear_cache() {
        let resolver = PropertyResolver::new();
        resolver.add_property("key".to_string(), "value".to_string());

        assert_eq!(resolver.resolve("${key}"), "value");

        resolver.clear_cache();

        assert_eq!(resolver.resolve("${key}"), "${key}");
    }

    // ========================================================================
    // FILE LOADING TESTS
    // ========================================================================

    /// Helper function to get test fixture path relative to workspace root
    fn fixture_path(relative_path: &str) -> String {
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        workspace_root
            .join("fixtures/java/test-properties")
            .join(relative_path)
            .to_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn test_load_properties_file() {
        let resolver = PropertyResolver::new();
        let file_path = fixture_path("application.properties");

        let count = resolver.load_from_file(&file_path).expect("Failed to load properties file");

        // Should load multiple properties
        assert!(count > 0, "Should load at least one property");

        // Test basic properties
        assert_eq!(
            resolver.resolve("${api.host}"),
            "http://localhost:8080"
        );
        assert_eq!(
            resolver.resolve("${api.version}"),
            "v1"
        );
        assert_eq!(
            resolver.resolve("${db.host}"),
            "localhost"
        );

        // Test nested property keys with dots
        assert_eq!(
            resolver.resolve("${payment.hub.host}"),
            "http://payment-service:8080"
        );
        assert_eq!(
            resolver.resolve("${payment.api.get_token}"),
            "api/v1/auth/token"
        );

        // Test multi-line value (backslash continuation)
        let description = resolver.resolve("${api.description}");
        assert!(description.contains("description"), "Multi-line property should be parsed");

        // Test boolean and numeric values
        assert_eq!(resolver.resolve("${feature.enabled}"), "true");
        assert_eq!(resolver.resolve("${max.connections}"), "100");
    }

    #[test]
    fn test_load_yaml_file() {
        let resolver = PropertyResolver::new();
        let file_path = fixture_path("application.yml");

        let count = resolver.load_from_file(&file_path).expect("Failed to load YAML file");

        assert!(count > 0, "Should load at least one property");

        // Test nested structure converted to dot notation
        assert_eq!(
            resolver.resolve("${api.host}"),
            "http://localhost:9090"
        );
        assert_eq!(
            resolver.resolve("${api.version}"),
            "v2"
        );
        assert_eq!(
            resolver.resolve("${api.auth.enabled}"),
            "true"
        );
        assert_eq!(
            resolver.resolve("${api.auth.type}"),
            "oauth2"
        );

        // Test deeply nested properties
        assert_eq!(
            resolver.resolve("${spring.application.name}"),
            "knowledge-graph"
        );
        assert_eq!(
            resolver.resolve("${spring.datasource.url}"),
            "jdbc:postgresql://localhost:5432/db"
        );

        // Test database pool properties
        assert_eq!(
            resolver.resolve("${database.pool.size}"),
            "50"
        );
        assert_eq!(
            resolver.resolve("${database.pool.maxWait}"),
            "10000"
        );

        // Test null value
        assert_eq!(
            resolver.resolve("${features.experimental}"),
            ""
        );
    }

    #[test]
    fn test_load_json_file() {
        let resolver = PropertyResolver::new();
        let file_path = fixture_path("config.json");

        let count = resolver.load_from_file(&file_path).expect("Failed to load JSON file");

        assert!(count > 0, "Should load at least one property");

        // Test nested JSON structure
        assert_eq!(
            resolver.resolve("${api.host}"),
            "http://json-api.example.com"
        );
        assert_eq!(
            resolver.resolve("${api.version}"),
            "v3"
        );
        assert_eq!(
            resolver.resolve("${api.endpoints.users}"),
            "/api/users"
        );
        assert_eq!(
            resolver.resolve("${api.endpoints.auth}"),
            "/api/auth"
        );

        // Test flat key with dots
        assert_eq!(
            resolver.resolve("${database.url}"),
            "mongodb://localhost:27017"
        );

        // Test boolean values
        assert_eq!(
            resolver.resolve("${cache.enabled}"),
            "true"
        );
        assert_eq!(
            resolver.resolve("${features.beta}"),
            "false"
        );

        // Test numeric value
        assert_eq!(
            resolver.resolve("${cache.ttl}"),
            "3600"
        );
    }

    #[test]
    fn test_load_multiple_files_with_override() {
        let resolver = PropertyResolver::new();

        let file_paths = vec![
            fixture_path("application.properties"),
            fixture_path("override.properties"),
        ];

        let count = resolver.load_from_files(&file_paths).expect("Failed to load files");

        assert!(count > 0, "Should load properties from both files");

        // Base property from first file
        assert_eq!(
            resolver.resolve("${api.version}"),
            "v1"
        );
        assert_eq!(
            resolver.resolve("${db.host}"),
            "localhost"
        );

        // Overridden property from second file
        assert_eq!(
            resolver.resolve("${api.host}"),
            "https://api.production.com",
            "Later file should override earlier property"
        );
        assert_eq!(
            resolver.resolve("${api.timeout}"),
            "10000",
            "Later file should override earlier property"
        );

        // New property from second file
        assert_eq!(
            resolver.resolve("${deployment.environment}"),
            "production"
        );
        assert_eq!(
            resolver.resolve("${deployment.region}"),
            "us-east-1"
        );
    }

    #[test]
    fn test_unsupported_file_format() {
        let resolver = PropertyResolver::new();

        // Try to load file with unsupported extension
        let result = resolver.load_from_file("test.xml");

        assert!(result.is_err(), "Should return error for unsupported format");

        if let Err(e) = result {
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            assert!(e.to_string().contains("Unsupported file format"));
        }
    }

    #[test]
    fn test_missing_file() {
        let resolver = PropertyResolver::new();

        let result = resolver.load_from_file(&fixture_path("nonexistent.properties"));

        assert!(result.is_err(), "Should return error for missing file");

        if let Err(e) = result {
            assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn test_properties_comments_and_empty_lines() {
        let resolver = PropertyResolver::new();

        // The application.properties fixture has comments and empty lines
        let count = resolver.load_from_file(&fixture_path("application.properties"))
            .expect("Failed to load properties");

        // Comments and empty lines should be skipped
        // Only actual properties should be counted
        assert!(count > 10, "Should load actual properties, skipping comments");

        // Verify properties are still accessible
        assert_eq!(
            resolver.resolve("${api.host}"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn test_resolve_with_loaded_properties() {
        let resolver = PropertyResolver::new();

        resolver.load_from_file(&fixture_path("application.properties"))
            .expect("Failed to load properties");

        // Test property placeholder resolution after loading from file
        assert_eq!(
            resolver.resolve("${payment.hub.host}/${payment.api.get_token}"),
            "http://payment-service:8080/api/v1/auth/token"
        );

        // Test with default value (should use loaded value)
        assert_eq!(
            resolver.resolve("${api.host:http://default.com}"),
            "http://localhost:8080"
        );

        // Test with default value for missing key
        assert_eq!(
            resolver.resolve("${missing.key:http://fallback.com}"),
            "http://fallback.com"
        );
    }

    #[test]
    fn test_yaml_deeply_nested_properties() {
        let resolver = PropertyResolver::new();

        resolver.load_from_file(&fixture_path("application.yml"))
            .expect("Failed to load YAML");

        // 3-level nesting: spring.datasource.username
        assert_eq!(
            resolver.resolve("${spring.datasource.username}"),
            "dbuser"
        );

        // 4-level nesting: database.pool.maxWait
        assert_eq!(
            resolver.resolve("${database.pool.maxWait}"),
            "10000"
        );
    }

    #[test]
    fn test_empty_property_value() {
        let resolver = PropertyResolver::new();

        resolver.load_from_file(&fixture_path("application.properties"))
            .expect("Failed to load properties");

        // Empty property value should resolve to empty string
        assert_eq!(
            resolver.resolve("${empty.property}"),
            ""
        );

        // Should still work in string interpolation
        assert_eq!(
            resolver.resolve("prefix-${empty.property}-suffix"),
            "prefix--suffix"
        );
    }
}
