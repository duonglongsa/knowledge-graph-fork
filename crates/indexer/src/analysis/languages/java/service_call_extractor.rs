use crate::analysis::types::ServiceCallNode;
use super::field_resolver::FieldResolver;
use super::property_resolver::PropertyResolver;
use parser_core::java::types::{JavaAnnotation, JavaDefinitionInfo};
use serde_json::Value;

/// Extracts external service call information from Java annotations
///
/// This extractor resolves field references and property placeholders in annotation values:
/// - Field references: `Config.BASE_URL` → `"http://localhost:8080"`
/// - Property placeholders: `"${api.host}"` → `"http://localhost:8080"`
/// - Combined: Field resolves to placeholder, then placeholder resolves to value
pub struct ServiceCallExtractor {
    field_resolver: FieldResolver,
    property_resolver: PropertyResolver,
}

impl ServiceCallExtractor {
    pub fn new() -> Self {
        Self {
            field_resolver: FieldResolver::new(),
            property_resolver: PropertyResolver::new(),
        }
    }

    /// Load property files for placeholder resolution
    ///
    /// # Arguments
    /// * `property_file_paths` - Paths to property files (.properties, .yml, .json)
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of properties loaded successfully
    /// * `Err(String)` - Error message if loading fails
    pub fn load_property_files(&self, property_file_paths: &[String]) -> Result<usize, String> {
        if property_file_paths.is_empty() {
            return Ok(0);
        }

        match self.property_resolver.load_from_files(property_file_paths) {
            Ok(count) => {
                log::info!(
                    "[SERVICE_CALL_EXTRACTOR] Loaded {} properties from {} file(s)",
                    count,
                    property_file_paths.len()
                );
                Ok(count)
            }
            Err(e) => {
                let error_msg = format!("Failed to load property files: {}", e);
                log::error!("[SERVICE_CALL_EXTRACTOR] {}", error_msg);
                Err(error_msg)
            }
        }
    }

    /// Extract Spring Boot context-path from loaded properties
    ///
    /// Checks for:
    /// 1. `server.servlet.context-path` (Spring Boot 2.x+)
    /// 2. `server.context-path` (Spring Boot 1.x)
    ///
    /// # Returns
    /// * `Some(String)` - Normalized context path (starts with /, no trailing /)
    /// * `None` - If no context path configured
    pub fn get_context_path(&self) -> Option<String> {
        // Try Spring Boot 2.x+ property first
        let context_path = self.property_resolver.resolve("${server.servlet.context-path}");
        if !context_path.starts_with("${") {
            return Some(Self::normalize_context_path(&context_path));
        }

        // Try Spring Boot 1.x property (deprecated but still supported)
        let context_path = self.property_resolver.resolve("${server.context-path}");
        if !context_path.starts_with("${") {
            return Some(Self::normalize_context_path(&context_path));
        }

        None
    }

    /// Normalize context path to ensure consistent format
    /// - Ensures it starts with /
    /// - Removes trailing /
    /// - Trims whitespace
    fn normalize_context_path(path: &str) -> String {
        let path = path.trim();

        if path.is_empty() {
            return String::new();
        }

        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        // Remove trailing slash
        path.trim_end_matches('/').to_string()
    }

    /// Resolve a value through field resolution → property resolution pipeline
    ///
    /// # Arguments
    /// * `value` - The original value from annotation (may be field reference or placeholder)
    /// * `definitions` - All Java definitions for field resolution
    /// * `context_package` - Package context for resolving simple field names
    ///
    /// # Returns
    /// Resolved value after applying both resolvers
    fn resolve_value(
        &self,
        value: &str,
        definitions: &[JavaDefinitionInfo],
        context_package: Option<&str>,
    ) -> String {
        log::debug!(
            "[SERVICE_CALL_EXTRACTOR] Resolving value: '{}' (definitions count: {}, package: {:?})",
            value,
            definitions.len(),
            context_package
        );

        // Stage 1: Field resolution
        if FieldResolver::is_field_reference(value) {
            if let Some(resolved) = self.field_resolver.resolve(value, definitions, context_package) {
                log::info!(
                    "[SERVICE_CALL_EXTRACTOR] Field resolved: '{}' -> '{}'",
                    value,
                    resolved
                );
                // Stage 2: Property resolution on field's value
                let final_value = self.property_resolver.resolve(&resolved);
                log::info!(
                    "[SERVICE_CALL_EXTRACTOR] Property resolved: '{}' -> '{}'",
                    resolved,
                    final_value
                );
                return final_value;
            } else {
                log::warn!(
                    "[SERVICE_CALL_EXTRACTOR] Field reference '{}' could not be resolved",
                    value
                );
            }
        }

        // Direct property resolution
        let final_value = self.property_resolver.resolve(value);
        if final_value != value {
            log::info!(
                "[SERVICE_CALL_EXTRACTOR] Property resolved: '{}' -> '{}'",
                value,
                final_value
            );
        }
        final_value
    }

    /// Extract service calls from annotations JSON string
    /// Returns a list of ServiceCallNode instances if valid service call annotations are found
    pub fn extract_service_calls_from_annotations(
        &self,
        annotations_json: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        definitions: &[JavaDefinitionInfo],
    ) -> Vec<ServiceCallNode> {
        let mut service_calls = Vec::new();

        // Parse class annotations
        let class_annotations: Vec<JavaAnnotation> = match serde_json::from_str(annotations_json)
        {
            Ok(annots) => annots,
            Err(e) => {
                log::warn!(
                    "[SERVICE_CALL_EXTRACTOR] Failed to parse annotations for {}: {}",
                    class_fqn,
                    e
                );
                return service_calls;
            }
        };

        // Extract package from class FQN for context
        let context_package = class_fqn.rsplitn(2, '.').nth(1).map(|s| s.to_string());

        // Check for @FeignClient annotation
        for annotation in &class_annotations {
            if annotation.name == "FeignClient" {
                if let Some(service_call) = self.create_service_call_from_feign_client(
                    annotation,
                    class_fqn,
                    class_name,
                    file_path,
                    start_line,
                    end_line,
                    definitions,
                    context_package.as_deref(),
                ) {
                    service_calls.push(service_call);
                }
            }
        }

        service_calls
    }

    /// Extract service calls from method annotations (for interfaces with @FeignClient)
    /// This is called for each method in a FeignClient interface
    pub fn extract_service_call_from_method(
        &self,
        method_annotations_json: &str,
        method_fqn: &str,
        method_name: &str,
        class_fqn: &str,
        class_name: &str,
        class_annotations_json: Option<&str>,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        definitions: &[JavaDefinitionInfo],
    ) -> Vec<ServiceCallNode> {
        let mut service_calls = Vec::new();

        // First check if the class has @FeignClient annotation
        let is_feign_client = if let Some(class_json) = class_annotations_json {
            Self::is_feign_client_interface(class_json)
        } else {
            false
        };

        if !is_feign_client {
            return service_calls;
        }

        // Extract package from class FQN for context
        let context_package = class_fqn.rsplitn(2, '.').nth(1).map(|s| s.to_string());

        // Extract base URL from @FeignClient annotation
        let (service_name_raw, service_url_raw) = if let Some(class_json) = class_annotations_json {
            Self::extract_feign_client_info(class_json)
        } else {
            (String::new(), String::new())
        };

        // Resolve service URL
        let service_url_resolved = self.resolve_value(
            &service_url_raw,
            definitions,
            context_package.as_deref(),
        );

        // Parse method annotations
        let method_annotations: Vec<JavaAnnotation> =
            match serde_json::from_str(method_annotations_json) {
                Ok(annots) => annots,
                Err(e) => {
                    log::warn!(
                        "[SERVICE_CALL_EXTRACTOR] Failed to parse method annotations for {}: {}",
                        method_fqn,
                        e
                    );
                    return service_calls;
                }
            };

        // Check each annotation for HTTP mapping annotations
        for annotation in &method_annotations {
            if let Some(service_call) = self.create_service_call_from_method_annotation(
                annotation,
                &service_name_raw,
                &service_url_raw,
                &service_url_resolved,
                method_name,
                method_fqn,
                class_name,
                class_fqn,
                file_path,
                start_line,
                end_line,
                definitions,
                context_package.as_deref(),
            ) {
                service_calls.push(service_call);
            }
        }

        service_calls
    }

    /// Check if class has @FeignClient annotation
    fn is_feign_client_interface(class_annotations_json: &str) -> bool {
        let class_annotations: Vec<JavaAnnotation> =
            match serde_json::from_str(class_annotations_json) {
                Ok(annots) => annots,
                Err(_) => return false,
            };

        class_annotations.iter().any(|a| a.name == "FeignClient")
    }

    /// Extract service name and URL from @FeignClient annotation
    fn extract_feign_client_info(class_annotations_json: &str) -> (String, String) {
        let class_annotations: Vec<JavaAnnotation> =
            match serde_json::from_str(class_annotations_json) {
                Ok(annots) => annots,
                Err(_) => return (String::new(), String::new()),
            };

        for annotation in class_annotations {
            if annotation.name == "FeignClient" {
                let service_name = Self::extract_string_argument(&annotation, "name")
                    .or_else(|| Self::extract_string_argument(&annotation, "value"))
                    .unwrap_or_default();

                let service_url = Self::extract_string_argument(&annotation, "url")
                    .unwrap_or_else(|| service_name.clone());

                return (service_name, service_url);
            }
        }

        (String::new(), String::new())
    }

    /// Create ServiceCallNode from @FeignClient annotation (class-level)
    fn create_service_call_from_feign_client(
        &self,
        annotation: &JavaAnnotation,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        definitions: &[JavaDefinitionInfo],
        context_package: Option<&str>,
    ) -> Option<ServiceCallNode> {
        // Extract service name from 'name' or 'value' argument
        let service_name = Self::extract_string_argument(annotation, "name")
            .or_else(|| Self::extract_string_argument(annotation, "value"))?;

        // Extract URL from 'url' argument (optional)
        let service_url_raw =
            Self::extract_string_argument(annotation, "url").unwrap_or_else(|| service_name.clone());

        // Resolve URL through pipeline
        let service_url_resolved = self.resolve_value(&service_url_raw, definitions, context_package);

        log::info!(
            "[SERVICE_CALL_EXTRACTOR] Extracted FeignClient: {} with URL {} (resolved: {}) from {}:{}",
            service_name,
            service_url_raw,
            service_url_resolved,
            file_path,
            start_line
        );

        // For class-level FeignClient, we create a generic service call entry
        Some(ServiceCallNode::new(
            "FeignClient".to_string(),
            service_name,
            Some(service_url_raw.clone()),  // original
            service_url_resolved.clone(),    // resolved
            "UNKNOWN".to_string(),           // Will be determined by method annotations
            Some("/".to_string()),           // original path
            "/".to_string(),                 // resolved path
            Some(service_url_raw.clone()),   // original full_path
            service_url_resolved,            // resolved full_path
            class_name.to_string(),
            class_fqn.to_string(),
            "".to_string(),                  // No specific method at class level
            class_fqn.to_string(),
            file_path.to_string(),
            start_line,
            end_line,
        ))
    }

    /// Create ServiceCallNode from method annotation in FeignClient interface
    #[allow(clippy::too_many_arguments)]
    fn create_service_call_from_method_annotation(
        &self,
        annotation: &JavaAnnotation,
        service_name: &str,
        service_url_raw: &str,
        service_url_resolved: &str,
        method_name: &str,
        method_fqn: &str,
        class_name: &str,
        class_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        definitions: &[JavaDefinitionInfo],
        context_package: Option<&str>,
    ) -> Option<ServiceCallNode> {
        // Check if this is an HTTP mapping annotation
        let http_method = Self::get_http_method(annotation)?;

        // Extract path from annotation
        let path_raw = Self::extract_path_from_annotation(annotation);

        // Resolve path through pipeline
        let path_resolved = self.resolve_value(&path_raw, definitions, context_package);

        // Build full paths
        let full_path_resolved = format!(
            "{}{}",
            service_url_resolved.trim_end_matches('/'),
            &path_resolved
        );
        let full_path_raw = format!(
            "{}{}",
            service_url_raw.trim_end_matches('/'),
            &path_raw
        );

        log::info!(
            "[SERVICE_CALL_EXTRACTOR] Extracted service call: {} {} (resolved: {}) from {}:{}",
            http_method,
            full_path_raw,
            full_path_resolved,
            file_path,
            start_line
        );

        Some(ServiceCallNode::new(
            "FeignClient".to_string(),
            service_name.to_string(),
            Some(service_url_raw.to_string()),  // original URL
            service_url_resolved.to_string(),    // resolved URL
            http_method,
            Some(path_raw),                      // original path
            path_resolved,                       // resolved path
            Some(full_path_raw),                 // original full path
            full_path_resolved,                  // resolved full path
            class_name.to_string(),
            class_fqn.to_string(),
            method_name.to_string(),
            method_fqn.to_string(),
            file_path.to_string(),
            start_line,
            end_line,
        ))
    }

    /// Extract HTTP method from annotation name and arguments
    /// For @RequestMapping, checks the 'method' argument to determine the actual HTTP method
    fn get_http_method(annotation: &JavaAnnotation) -> Option<String> {
        match annotation.name.as_str() {
            "GetMapping" => Some("GET".to_string()),
            "PostMapping" => Some("POST".to_string()),
            "PutMapping" => Some("PUT".to_string()),
            "DeleteMapping" => Some("DELETE".to_string()),
            "PatchMapping" => Some("PATCH".to_string()),
            "RequestMapping" => {
                // Check for explicit method argument
                Self::extract_http_method_from_arguments(&annotation.arguments)
            }
            _ => None,
        }
    }

    /// Extract HTTP method from @RequestMapping method argument
    fn extract_http_method_from_arguments(arguments: &[parser_core::java::types::JavaAnnotationArgument]) -> Option<String> {
        for arg in arguments {
            if arg.name.as_deref() == Some("method") {
                let value = arg.value.to_uppercase();
                if value.contains("GET") {
                    return Some("GET".to_string());
                } else if value.contains("POST") {
                    return Some("POST".to_string());
                } else if value.contains("PUT") {
                    return Some("PUT".to_string());
                } else if value.contains("DELETE") {
                    return Some("DELETE".to_string());
                } else if value.contains("PATCH") {
                    return Some("PATCH".to_string());
                }
            }
        }
        // Default to GET if method not specified
        Some("GET".to_string())
    }

    /// Extract path from annotation arguments
    fn extract_path_from_annotation(annotation: &JavaAnnotation) -> String {
        for arg in &annotation.arguments {
            // Check for value, path, or unnamed argument
            if arg.name.is_none()
                || arg.name.as_deref() == Some("value")
                || arg.name.as_deref() == Some("path")
            {
                // Parse the value as JSON to handle arrays
                if let Ok(Value::String(path)) = serde_json::from_str(&arg.value) {
                    let cleaned_path = if path.starts_with('/') {
                        path
                    } else {
                        format!("/{}", path)
                    };
                    return cleaned_path;
                }
                // Handle array of paths - take the first one
                if let Ok(Value::Array(paths)) = serde_json::from_str(&arg.value) {
                    if let Some(Value::String(first_path)) = paths.first() {
                        let cleaned_path = if first_path.starts_with('/') {
                            first_path.clone()
                        } else {
                            format!("/{}", first_path)
                        };
                        return cleaned_path;
                    }
                }
                // Try direct string value
                let value_str = arg.value.trim_matches('"');
                if !value_str.is_empty() {
                    let cleaned_path = if value_str.starts_with('/') {
                        value_str.to_string()
                    } else {
                        format!("/{}", value_str)
                    };
                    return cleaned_path;
                }
            }
        }

        // Default path if not specified
        "/".to_string()
    }

    /// Extract string argument from annotation
    fn extract_string_argument(annotation: &JavaAnnotation, arg_name: &str) -> Option<String> {
        for arg in &annotation.arguments {
            if arg.name.as_deref() == Some(arg_name)
                || (arg_name == "value" && arg.name.is_none())
            {
                // Try parsing as JSON string
                if let Ok(Value::String(s)) = serde_json::from_str(&arg.value) {
                    return Some(s);
                }
                // Try direct string value
                let value_str = arg.value.trim_matches('"');
                if !value_str.is_empty() {
                    return Some(value_str.to_string());
                }
            }
        }
        None
    }
}

impl Default for ServiceCallExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parser_core::java::types::JavaAnnotationArgument;

    #[test]
    fn test_get_http_method() {
        let get_mapping = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&get_mapping),
            Some("GET".to_string())
        );

        let post_mapping = JavaAnnotation {
            name: "PostMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&post_mapping),
            Some("POST".to_string())
        );

        let put_mapping = JavaAnnotation {
            name: "PutMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&put_mapping),
            Some("PUT".to_string())
        );

        let delete_mapping = JavaAnnotation {
            name: "DeleteMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&delete_mapping),
            Some("DELETE".to_string())
        );

        let patch_mapping = JavaAnnotation {
            name: "PatchMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&patch_mapping),
            Some("PATCH".to_string())
        );

        let other = JavaAnnotation {
            name: "Other".to_string(),
            arguments: vec![],
        };
        assert_eq!(ServiceCallExtractor::get_http_method(&other), None);

        // Test @RequestMapping with explicit POST method
        let request_mapping_post = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![JavaAnnotationArgument {
                name: Some("method".to_string()),
                value: "RequestMethod.POST".to_string(),
            }],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&request_mapping_post),
            Some("POST".to_string())
        );

        // Test @RequestMapping without method (defaults to GET)
        let request_mapping_default = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            ServiceCallExtractor::get_http_method(&request_mapping_default),
            Some("GET".to_string())
        );
    }

    #[test]
    fn test_extract_feign_client_info() {
        let annotations_json = r#"[
            {
                "name": "FeignClient",
                "arguments": [
                    {"name": "name", "value": "\"user-service\""},
                    {"name": "url", "value": "\"http://localhost:8080\""}
                ]
            }
        ]"#;

        let (service_name, service_url) =
            ServiceCallExtractor::extract_feign_client_info(annotations_json);
        assert_eq!(service_name, "user-service");
        assert_eq!(service_url, "http://localhost:8080");
    }

    #[test]
    fn test_is_feign_client_interface() {
        let annotations_json = r#"[
            {"name": "FeignClient", "arguments": [{"name": "name", "value": "\"service\""}]}
        ]"#;

        assert!(ServiceCallExtractor::is_feign_client_interface(
            annotations_json
        ));

        let non_feign_json = r#"[{"name": "RestController", "arguments": []}]"#;
        assert!(!ServiceCallExtractor::is_feign_client_interface(
            non_feign_json
        ));
    }

    #[test]
    fn test_extract_path_from_annotation() {
        let annotation = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![JavaAnnotationArgument {
                name: Some("value".to_string()),
                value: "\"/users/{id}\"".to_string(),
            }],
        };

        let path = ServiceCallExtractor::extract_path_from_annotation(&annotation);
        assert_eq!(path, "/users/{id}");
    }
}
