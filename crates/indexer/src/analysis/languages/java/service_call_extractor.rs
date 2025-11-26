use crate::analysis::types::ServiceCallNode;
use parser_core::java::types::JavaAnnotation;
use serde_json::Value;

/// Extracts external service call information from Java annotations
pub struct ServiceCallExtractor;

impl ServiceCallExtractor {
    /// Extract service calls from annotations JSON string
    /// Returns a list of ServiceCallNode instances if valid service call annotations are found
    pub fn extract_service_calls_from_annotations(
        annotations_json: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
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

        // Check for @FeignClient annotation
        for annotation in &class_annotations {
            if annotation.name == "FeignClient" {
                if let Some(service_call) = Self::create_service_call_from_feign_client(
                    annotation,
                    class_fqn,
                    class_name,
                    file_path,
                    start_line,
                    end_line,
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
        method_annotations_json: &str,
        method_fqn: &str,
        method_name: &str,
        class_fqn: &str,
        class_name: &str,
        class_annotations_json: Option<&str>,
        file_path: &str,
        start_line: i32,
        end_line: i32,
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

        // Extract base URL from @FeignClient annotation
        let (service_name, service_url) = if let Some(class_json) = class_annotations_json {
            Self::extract_feign_client_info(class_json)
        } else {
            (String::new(), String::new())
        };

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
            if let Some(service_call) = Self::create_service_call_from_method_annotation(
                annotation,
                &service_name,
                &service_url,
                method_name,
                method_fqn,
                class_name,
                class_fqn,
                file_path,
                start_line,
                end_line,
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
        annotation: &JavaAnnotation,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
    ) -> Option<ServiceCallNode> {
        // Extract service name from 'name' or 'value' argument
        let service_name = Self::extract_string_argument(annotation, "name")
            .or_else(|| Self::extract_string_argument(annotation, "value"))?;

        // Extract URL from 'url' argument (optional)
        let service_url =
            Self::extract_string_argument(annotation, "url").unwrap_or_else(|| service_name.clone());

        log::info!(
            "[SERVICE_CALL_EXTRACTOR] Extracted FeignClient: {} with URL {} from {}:{}",
            service_name,
            service_url,
            file_path,
            start_line
        );

        // For class-level FeignClient, we create a generic service call entry
        Some(ServiceCallNode::new(
            "FeignClient".to_string(),
            service_name,
            service_url.clone(),
            "UNKNOWN".to_string(), // Will be determined by method annotations
            "/".to_string(),
            service_url,
            class_name.to_string(),
            class_fqn.to_string(),
            "".to_string(), // No specific method at class level
            class_fqn.to_string(),
            file_path.to_string(),
            start_line,
            end_line,
        ))
    }

    /// Create ServiceCallNode from method annotation in FeignClient interface
    fn create_service_call_from_method_annotation(
        annotation: &JavaAnnotation,
        service_name: &str,
        service_url: &str,
        method_name: &str,
        method_fqn: &str,
        class_name: &str,
        class_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
    ) -> Option<ServiceCallNode> {
        // Check if this is an HTTP mapping annotation
        let http_method = Self::get_http_method(&annotation.name)?;

        // Extract path from annotation
        let path = Self::extract_path_from_annotation(annotation);

        // Build full path
        let full_path = format!("{}{}", service_url.trim_end_matches('/'), &path);

        log::info!(
            "[SERVICE_CALL_EXTRACTOR] Extracted service call: {} {} from {}:{}",
            http_method,
            full_path,
            file_path,
            start_line
        );

        Some(ServiceCallNode::new(
            "FeignClient".to_string(),
            service_name.to_string(),
            service_url.to_string(),
            http_method,
            path,
            full_path,
            class_name.to_string(),
            class_fqn.to_string(),
            method_name.to_string(),
            method_fqn.to_string(),
            file_path.to_string(),
            start_line,
            end_line,
        ))
    }

    /// Extract HTTP method from annotation name
    fn get_http_method(annotation_name: &str) -> Option<String> {
        match annotation_name {
            "GetMapping" => Some("GET".to_string()),
            "PostMapping" => Some("POST".to_string()),
            "PutMapping" => Some("PUT".to_string()),
            "DeleteMapping" => Some("DELETE".to_string()),
            "PatchMapping" => Some("PATCH".to_string()),
            "RequestMapping" => Some("GET".to_string()), // Default to GET for @RequestMapping
            _ => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use parser_core::java::types::JavaAnnotationArgument;

    #[test]
    fn test_get_http_method() {
        assert_eq!(
            ServiceCallExtractor::get_http_method("GetMapping"),
            Some("GET".to_string())
        );
        assert_eq!(
            ServiceCallExtractor::get_http_method("PostMapping"),
            Some("POST".to_string())
        );
        assert_eq!(
            ServiceCallExtractor::get_http_method("PutMapping"),
            Some("PUT".to_string())
        );
        assert_eq!(
            ServiceCallExtractor::get_http_method("DeleteMapping"),
            Some("DELETE".to_string())
        );
        assert_eq!(
            ServiceCallExtractor::get_http_method("PatchMapping"),
            Some("PATCH".to_string())
        );
        assert_eq!(ServiceCallExtractor::get_http_method("Other"), None);
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
