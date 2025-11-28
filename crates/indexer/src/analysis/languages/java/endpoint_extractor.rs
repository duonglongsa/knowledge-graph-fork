use crate::analysis::types::EndpointNode;
use parser_core::java::types::{JavaAnnotation, JavaDefinitionInfo};
use serde_json::Value;
use super::constant_resolver::ConstantResolver;

/// Extracts API endpoint information from Java annotations
pub struct EndpointExtractor;

/// Context for endpoint extraction with constant resolution support
pub struct ExtractionContext<'a> {
    pub constant_resolver: &'a ConstantResolver,
    pub all_definitions: &'a [JavaDefinitionInfo],
}

impl EndpointExtractor {
    /// Extract endpoints from annotations JSON string
    /// Returns a list of EndpointNode instances if valid endpoint annotations are found
    pub fn extract_endpoints_from_annotations(
        annotations_json: &str,
        method_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        class_annotations_json: Option<&str>,
    ) -> Vec<EndpointNode> {
        let mut endpoints = Vec::new();

        // Parse method annotations
        let method_annotations: Vec<JavaAnnotation> = match serde_json::from_str(annotations_json)
        {
            Ok(annots) => annots,
            Err(e) => {
                log::warn!(
                    "[ENDPOINT_EXTRACTOR] Failed to parse annotations for {}: {}",
                    method_fqn,
                    e
                );
                return endpoints;
            }
        };

        // Parse class annotations to get base path from @RequestMapping
        let class_base_path = if let Some(class_json) = class_annotations_json {
            Self::extract_base_path_from_class(class_json)
        } else {
            None
        };

        // Check each annotation for endpoint mappings
        for annotation in &method_annotations {
            if let Some(endpoint) = Self::create_endpoint_from_annotation(
                annotation,
                &class_base_path,
                method_fqn,
                file_path,
                start_line,
                end_line,
                &method_annotations,
            ) {
                endpoints.push(endpoint);
            }
        }

        endpoints
    }

    /// Extract endpoints from annotations with constant resolution support
    /// Returns a list of EndpointNode instances if valid endpoint annotations are found
    pub fn extract_endpoints_with_context(
        annotations_json: &str,
        method_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        class_annotations_json: Option<&str>,
        context: &ExtractionContext,
    ) -> Vec<EndpointNode> {
        let mut endpoints = Vec::new();

        // Parse method annotations
        let method_annotations: Vec<JavaAnnotation> = match serde_json::from_str(annotations_json)
        {
            Ok(annots) => annots,
            Err(e) => {
                log::warn!(
                    "[ENDPOINT_EXTRACTOR] Failed to parse annotations for {}: {}",
                    method_fqn,
                    e
                );
                return endpoints;
            }
        };

        // Parse class annotations to get base path from @RequestMapping
        let class_base_path = if let Some(class_json) = class_annotations_json {
            Self::extract_base_path_from_class_with_context(class_json, Some(context))
        } else {
            None
        };

        // Check each annotation for endpoint mappings
        for annotation in &method_annotations {
            if let Some(endpoint) = Self::create_endpoint_from_annotation_with_context(
                annotation,
                &class_base_path,
                method_fqn,
                file_path,
                start_line,
                end_line,
                &method_annotations,
                Some(context),
            ) {
                endpoints.push(endpoint);
            }
        }

        endpoints
    }

    /// Extract base path from @RequestMapping on class
    fn extract_base_path_from_class(class_annotations_json: &str) -> Option<String> {
        Self::extract_base_path_from_class_with_context(class_annotations_json, None)
    }

    /// Extract base path from @RequestMapping on class with optional constant resolution
    fn extract_base_path_from_class_with_context(
        class_annotations_json: &str,
        context: Option<&ExtractionContext>,
    ) -> Option<String> {
        let class_annotations: Vec<JavaAnnotation> =
            serde_json::from_str(class_annotations_json).ok()?;

        for annotation in class_annotations {
            if annotation.name == "RequestMapping" {
                if let Some(path) = Self::extract_path_from_annotation_with_context(&annotation, context) {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Create EndpointNode from a single annotation
    fn create_endpoint_from_annotation(
        annotation: &JavaAnnotation,
        class_base_path: &Option<String>,
        _method_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        all_annotations: &[JavaAnnotation],
    ) -> Option<EndpointNode> {
        // Check if this is an endpoint mapping annotation
        let http_method = Self::get_http_method(&annotation.name)?;

        // Extract path from annotation
        let path = Self::extract_path_from_annotation(annotation)?;

        // Build full path with class base path
        let full_path = if let Some(base) = class_base_path {
            format!("{}{}", base.trim_end_matches('/'), &path)
        } else {
            path.clone()
        };

        // Check if deprecated
        let deprecated = Self::is_deprecated(all_annotations);

        // Extract consumes and produces
        let consumes = Self::extract_array_argument(annotation, "consumes");
        let produces = Self::extract_array_argument(annotation, "produces");

        log::info!(
            "[ENDPOINT_EXTRACTOR] Extracted endpoint: {} {} from {}:{}",
            http_method,
            full_path,
            file_path,
            start_line
        );

        Some(EndpointNode {
            http_method,
            path,
            full_path,
            consumes,
            produces,
            description: None, // Could be extracted from JavaDoc in future
            deprecated,
            path_params_json: None, // Could be extracted from @PathVariable in future
            query_params_json: None, // Could be extracted from @RequestParam in future
            request_body_json: None, // Could be extracted from @RequestBody in future
            response_body_json: None, // Could be extracted from return type in future
            file_path: file_path.to_string(),
            start_line,
            end_line,
        })
    }

    /// Create EndpointNode from a single annotation with optional constant resolution
    fn create_endpoint_from_annotation_with_context(
        annotation: &JavaAnnotation,
        class_base_path: &Option<String>,
        _method_fqn: &str,
        file_path: &str,
        start_line: i32,
        end_line: i32,
        all_annotations: &[JavaAnnotation],
        context: Option<&ExtractionContext>,
    ) -> Option<EndpointNode> {
        // Check if this is an endpoint mapping annotation
        let http_method = Self::get_http_method(&annotation.name)?;

        // Extract path from annotation with constant resolution
        let path = Self::extract_path_from_annotation_with_context(annotation, context)?;

        // Build full path with class base path
        let full_path = if let Some(base) = class_base_path {
            format!("{}{}", base.trim_end_matches('/'), &path)
        } else {
            path.clone()
        };

        // Check if deprecated
        let deprecated = Self::is_deprecated(all_annotations);

        // Extract consumes and produces
        let consumes = Self::extract_array_argument(annotation, "consumes");
        let produces = Self::extract_array_argument(annotation, "produces");

        log::info!(
            "[ENDPOINT_EXTRACTOR] Extracted endpoint: {} {} from {}:{}",
            http_method,
            full_path,
            file_path,
            start_line
        );

        Some(EndpointNode {
            http_method,
            path,
            full_path,
            consumes,
            produces,
            description: None,
            deprecated,
            path_params_json: None,
            query_params_json: None,
            request_body_json: None,
            response_body_json: None,
            file_path: file_path.to_string(),
            start_line,
            end_line,
        })
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

    /// Extract path from annotation arguments (without constant resolution)
    fn extract_path_from_annotation(annotation: &JavaAnnotation) -> Option<String> {
        Self::extract_path_from_annotation_with_context(annotation, None)
    }

    /// Extract path from annotation arguments with optional constant resolution
    fn extract_path_from_annotation_with_context(
        annotation: &JavaAnnotation,
        context: Option<&ExtractionContext>,
    ) -> Option<String> {
        for arg in &annotation.arguments {
            // Check for value, path, or unnamed argument
            if arg.name.is_none() || arg.name.as_deref() == Some("value") || arg.name.as_deref() == Some("path") {
                // Parse the value as JSON to handle arrays
                if let Ok(Value::String(path)) = serde_json::from_str(&arg.value) {
                    let cleaned_path = if path.starts_with('/') {
                        path
                    } else {
                        format!("/{}", path)
                    };
                    return Some(cleaned_path);
                }
                // Handle array of paths - take the first one
                if let Ok(Value::Array(paths)) = serde_json::from_str(&arg.value) {
                    if let Some(Value::String(first_path)) = paths.first() {
                        let cleaned_path = if first_path.starts_with('/') {
                            first_path.clone()
                        } else {
                            format!("/{}", first_path)
                        };
                        return Some(cleaned_path);
                    }
                }

                // Try direct string value
                let value_str = arg.value.trim_matches('"');

                // NEW: Try constant resolution if context is available
                if let Some(ctx) = context {
                    if ConstantResolver::is_constant_reference(value_str) {
                        if let Some(resolved) = ctx.constant_resolver.resolve(value_str, ctx.all_definitions) {
                            log::info!(
                                "[ENDPOINT_EXTRACTOR] Resolved constant {} -> {}",
                                value_str,
                                resolved
                            );
                            let cleaned_path = if resolved.starts_with('/') {
                                resolved
                            } else {
                                format!("/{}", resolved)
                            };
                            return Some(cleaned_path);
                        } else {
                            // Could not resolve - use placeholder
                            log::warn!(
                                "[ENDPOINT_EXTRACTOR] Could not resolve constant reference: {}",
                                value_str
                            );
                            return Some(format!("{{{}}}", value_str));
                        }
                    }
                }

                // Fallback: use value as-is
                if !value_str.is_empty() {
                    let cleaned_path = if value_str.starts_with('/') {
                        value_str.to_string()
                    } else {
                        format!("/{}", value_str)
                    };
                    return Some(cleaned_path);
                }
            }
        }

        // Default path if not specified
        Some("/".to_string())
    }

    /// Extract array argument (like consumes or produces)
    fn extract_array_argument(annotation: &JavaAnnotation, arg_name: &str) -> Option<String> {
        for arg in &annotation.arguments {
            if arg.name.as_deref() == Some(arg_name) {
                // If it's already a JSON array, return it
                if arg.value.starts_with('[') {
                    return Some(arg.value.clone());
                }
                // If it's a single value, wrap it in an array
                return Some(format!("[{}]", arg.value));
            }
        }
        None
    }

    /// Check if method is deprecated
    fn is_deprecated(annotations: &[JavaAnnotation]) -> bool {
        annotations.iter().any(|a| a.name == "Deprecated")
    }

    /// Check if class is a Controller (has @RestController, @Controller, or @RequestMapping)
    /// This helps distinguish Controllers from FeignClient interfaces
    pub fn is_controller_class(class_annotations_json: Option<&str>) -> bool {
        if let Some(class_json) = class_annotations_json {
            if let Ok(class_annotations) = serde_json::from_str::<Vec<JavaAnnotation>>(class_json) {
                return class_annotations.iter().any(|a| {
                    matches!(
                        a.name.as_str(),
                        "RestController" | "Controller" | "RequestMapping"
                    )
                });
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_http_method() {
        assert_eq!(
            EndpointExtractor::get_http_method("GetMapping"),
            Some("GET".to_string())
        );
        assert_eq!(
            EndpointExtractor::get_http_method("PostMapping"),
            Some("POST".to_string())
        );
        assert_eq!(
            EndpointExtractor::get_http_method("PutMapping"),
            Some("PUT".to_string())
        );
        assert_eq!(
            EndpointExtractor::get_http_method("DeleteMapping"),
            Some("DELETE".to_string())
        );
        assert_eq!(
            EndpointExtractor::get_http_method("PatchMapping"),
            Some("PATCH".to_string())
        );
        assert_eq!(EndpointExtractor::get_http_method("Other"), None);
    }

    #[test]
    fn test_is_deprecated() {
        let annotations = vec![
            JavaAnnotation {
                name: "GetMapping".to_string(),
                arguments: vec![],
            },
            JavaAnnotation {
                name: "Deprecated".to_string(),
                arguments: vec![],
            },
        ];
        assert!(EndpointExtractor::is_deprecated(&annotations));

        let annotations_no_deprecated = vec![JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![],
        }];
        assert!(!EndpointExtractor::is_deprecated(&annotations_no_deprecated));
    }
}
