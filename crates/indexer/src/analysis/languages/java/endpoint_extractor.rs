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
    /// Spring Boot context-path from configuration (e.g., "/retail-api")
    pub context_path: Option<&'a str>,
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
    ///
    /// **Supports multiple paths:**
    /// - Class-level: @RequestMapping({"/api/v1", "/api/v2"})
    /// - Method-level: @GetMapping({"/user", "/users"})
    /// - Cartesian product: Creates all combinations (e.g., 2 class paths × 2 method paths = 4 endpoints)
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

        // Extract ALL base paths from class-level @RequestMapping
        let class_base_paths = if let Some(class_json) = class_annotations_json {
            Self::extract_base_paths_from_class_with_context(class_json, Some(context))
        } else {
            vec![]
        };

        // Check each annotation for endpoint mappings
        for annotation in &method_annotations {
            // Extract ALL HTTP methods (supports @RequestMapping without method argument)
            let http_methods = Self::get_http_methods(annotation);
            if http_methods.is_empty() {
                continue; // Not an endpoint mapping annotation
            }

            // Extract ALL method-level paths
            let method_paths = Self::extract_paths_from_annotation_with_context(annotation, Some(context));

            // Check if deprecated
            let deprecated = Self::is_deprecated(&method_annotations);

            // Extract consumes and produces
            let consumes = Self::extract_array_argument(annotation, "consumes");
            let produces = Self::extract_array_argument(annotation, "produces");

            // Create endpoints for Cartesian product of http_methods × class_base_paths × method_paths
            for http_method in &http_methods {
                if class_base_paths.is_empty() {
                    // No class base path - create endpoints with method paths only
                    for method_path in &method_paths {
                        let full_path = Self::build_full_path(
                            context.context_path,
                            None,
                            method_path
                        );

                        log::info!(
                            "[ENDPOINT_EXTRACTOR] Extracted endpoint: {} {} (context: {:?}) from {}:{}",
                            http_method,
                            full_path,
                            context.context_path,
                            file_path,
                            start_line
                        );

                        endpoints.push(EndpointNode {
                            http_method: http_method.clone(),
                            path: method_path.clone(),
                            full_path,
                            context_path: context.context_path.map(String::from),
                            consumes: consumes.clone(),
                            produces: produces.clone(),
                            description: None,
                            deprecated,
                            path_params_json: None,
                            query_params_json: None,
                            request_body_json: None,
                            response_body_json: None,
                            file_path: file_path.to_string(),
                            start_line,
                            end_line,
                        });
                    }
                } else {
                    // Has class base paths - create Cartesian product
                    for class_base_path in &class_base_paths {
                        for method_path in &method_paths {
                            let full_path = Self::build_full_path(
                                context.context_path,
                                Some(class_base_path),
                                method_path
                            );

                            log::info!(
                                "[ENDPOINT_EXTRACTOR] Extracted endpoint: {} {} (context: {:?}, base: {}, method: {}) from {}:{}",
                                http_method,
                                full_path,
                                context.context_path,
                                class_base_path,
                                method_path,
                                file_path,
                                start_line
                            );

                            endpoints.push(EndpointNode {
                                http_method: http_method.clone(),
                                path: method_path.clone(),
                                full_path,
                                context_path: context.context_path.map(String::from),
                                consumes: consumes.clone(),
                                produces: produces.clone(),
                                description: None,
                                deprecated,
                                path_params_json: None,
                                query_params_json: None,
                                request_body_json: None,
                                response_body_json: None,
                                file_path: file_path.to_string(),
                                start_line,
                                end_line,
                            });
                        }
                    }
                }
            }
        }

        endpoints
    }

    /// Extract base path from @RequestMapping on class
    fn extract_base_path_from_class(class_annotations_json: &str) -> Option<String> {
        // For backward compatibility, return the first path
        Self::extract_base_paths_from_class_with_context(class_annotations_json, None)
            .into_iter()
            .next()
    }

    /// Extract ALL base paths from @RequestMapping on class (supports multiple paths)
    /// Returns a vector of all paths found in the @RequestMapping value/path argument
    fn extract_base_paths_from_class_with_context(
        class_annotations_json: &str,
        context: Option<&ExtractionContext>,
    ) -> Vec<String> {
        let class_annotations: Vec<JavaAnnotation> =
            match serde_json::from_str(class_annotations_json) {
                Ok(annots) => annots,
                Err(_) => return vec![],
            };

        for annotation in class_annotations {
            if annotation.name == "RequestMapping" {
                let paths = Self::extract_paths_from_annotation_with_context(&annotation, context);
                if !paths.is_empty() {
                    return paths;
                }
            }
        }

        vec![]
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
        let http_method = Self::get_http_method(annotation)?;

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
            context_path: None,
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
        let http_method = Self::get_http_method(annotation)?;

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
            context_path: context.and_then(|ctx| ctx.context_path.map(String::from)),
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

    /// Extract ALL HTTP methods from annotation
    /// For @RequestMapping without method argument, returns all standard HTTP methods
    /// For @RequestMapping with method argument, returns specified method(s)
    /// For specific mappings (@GetMapping, etc.), returns single method
    fn get_http_methods(annotation: &JavaAnnotation) -> Vec<String> {
        match annotation.name.as_str() {
            "GetMapping" => vec!["GET".to_string()],
            "PostMapping" => vec!["POST".to_string()],
            "PutMapping" => vec!["PUT".to_string()],
            "DeleteMapping" => vec!["DELETE".to_string()],
            "PatchMapping" => vec!["PATCH".to_string()],
            "RequestMapping" => {
                // Check for explicit method argument
                Self::extract_http_methods_from_arguments(&annotation.arguments)
            }
            _ => vec![],
        }
    }

    /// Extract ALL HTTP methods from @RequestMapping method argument
    /// Supports:
    /// - Single method: method = RequestMethod.POST
    /// - Multiple methods: method = {RequestMethod.GET, RequestMethod.POST}
    /// - No method argument: returns all standard HTTP methods (GET, POST, PUT, DELETE, PATCH)
    fn extract_http_methods_from_arguments(arguments: &[parser_core::java::types::JavaAnnotationArgument]) -> Vec<String> {
        for arg in arguments {
            if arg.name.as_deref() == Some("method") {
                let value = arg.value.to_uppercase();
                let mut methods = Vec::new();

                // Parse array syntax: {RequestMethod.GET, RequestMethod.POST}
                if value.contains('{') {
                    // Extract all methods from array
                    if value.contains("GET") {
                        methods.push("GET".to_string());
                    }
                    if value.contains("POST") {
                        methods.push("POST".to_string());
                    }
                    if value.contains("PUT") {
                        methods.push("PUT".to_string());
                    }
                    if value.contains("DELETE") {
                        methods.push("DELETE".to_string());
                    }
                    if value.contains("PATCH") {
                        methods.push("PATCH".to_string());
                    }
                    if !methods.is_empty() {
                        return methods;
                    }
                } else {
                    // Single method
                    if value.contains("GET") {
                        return vec!["GET".to_string()];
                    } else if value.contains("POST") {
                        return vec!["POST".to_string()];
                    } else if value.contains("PUT") {
                        return vec!["PUT".to_string()];
                    } else if value.contains("DELETE") {
                        return vec!["DELETE".to_string()];
                    } else if value.contains("PATCH") {
                        return vec!["PATCH".to_string()];
                    }
                }
            }
        }

        // No method argument specified - return all standard HTTP methods
        // This matches Spring's behavior where @RequestMapping without method handles all methods
        vec![
            "GET".to_string(),
            "POST".to_string(),
            "PUT".to_string(),
            "DELETE".to_string(),
            "PATCH".to_string(),
        ]
    }

    /// Extract path from annotation arguments (without constant resolution)
    /// Returns the first path found for backward compatibility
    fn extract_path_from_annotation(annotation: &JavaAnnotation) -> Option<String> {
        Self::extract_paths_from_annotation_with_context(annotation, None)
            .into_iter()
            .next()
    }

    /// Extract path from annotation arguments with optional constant resolution
    /// Returns the first path found for backward compatibility
    fn extract_path_from_annotation_with_context(
        annotation: &JavaAnnotation,
        context: Option<&ExtractionContext>,
    ) -> Option<String> {
        Self::extract_paths_from_annotation_with_context(annotation, context)
            .into_iter()
            .next()
    }

    /// Extract ALL paths from annotation arguments with optional constant resolution
    /// Supports:
    /// - Single path: @GetMapping("/api")
    /// - Multiple paths: @GetMapping({"/api", "/v1/api"})
    /// - Named arguments: value=..., path=...
    /// - Constant references: @GetMapping(MyConstants.BASE_PATH)
    fn extract_paths_from_annotation_with_context(
        annotation: &JavaAnnotation,
        context: Option<&ExtractionContext>,
    ) -> Vec<String> {
        let mut paths = Vec::new();

        for arg in &annotation.arguments {
            // Check for value, path, or unnamed argument
            if arg.name.is_none() || arg.name.as_deref() == Some("value") || arg.name.as_deref() == Some("path") {
                log::debug!(
                    "[ENDPOINT_EXTRACTOR] Parsing path argument: name={:?}, value={}",
                    arg.name,
                    arg.value
                );
                
                // Normalize Java array syntax {a, b} to JSON array [a, b]
                let normalized_value = if arg.value.trim().starts_with('{') && arg.value.trim().ends_with('}') {
                    arg.value.replace('{', "[").replace('}', "]")
                } else {
                    arg.value.clone()
                };
                
                // Parse the value as JSON to handle arrays
                if let Ok(Value::String(path)) = serde_json::from_str(&normalized_value) {
                    let cleaned_path = if path.starts_with('/') {
                        path
                    } else {
                        format!("/{}", path)
                    };
                    paths.push(cleaned_path);
                    return paths; // Found single string, return immediately
                }

                // Handle array of paths - extract ALL paths
                if let Ok(Value::Array(path_array)) = serde_json::from_str(&normalized_value) {
                    for path_value in path_array {
                        if let Value::String(path) = path_value {
                            let cleaned_path = if path.starts_with('/') {
                                path
                            } else {
                                format!("/{}", path)
                            };
                            paths.push(cleaned_path);
                        }
                    }
                    if !paths.is_empty() {
                        return paths; // Found array, return all paths
                    }
                }

                // Try direct string value
                let value_str = arg.value.trim_matches('"');

                // Try constant resolution if context is available
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
                            paths.push(cleaned_path);
                            return paths;
                        } else {
                            // Could not resolve - use placeholder
                            log::warn!(
                                "[ENDPOINT_EXTRACTOR] Could not resolve constant reference: {}",
                                value_str
                            );
                            paths.push(format!("{{{}}}", value_str));
                            return paths;
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
                    paths.push(cleaned_path);
                    return paths;
                }
            }
        }

        // Default path if not specified
        if paths.is_empty() {
            paths.push("/".to_string());
        }

        paths
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

    /// Build full path from context_path + class_base_path + method_path
    ///
    /// # Arguments
    /// * `context_path` - Spring Boot context-path from config (e.g., "/retail-api")
    /// * `class_base_path` - Class-level @RequestMapping path (e.g., "/advertisementms")
    /// * `method_path` - Method-level mapping path (e.g., "/getMenuTree")
    ///
    /// # Returns
    /// Combined path ensuring no double slashes and proper leading slash
    ///
    /// # Examples
    /// - context="/api", base="/users", method="/list" → "/api/users/list"
    /// - context=None, base="/users", method="/list" → "/users/list"
    /// - context="/api", base=None, method="/list" → "/api/list"
    fn build_full_path(
        context_path: Option<&str>,
        class_base_path: Option<&str>,
        method_path: &str,
    ) -> String {
        let mut parts = Vec::new();

        // Add context path (e.g., "/retail-api")
        if let Some(ctx) = context_path {
            if !ctx.is_empty() {
                parts.push(ctx.trim_end_matches('/'));
            }
        }

        // Add class base path (e.g., "/advertisementms")
        if let Some(base) = class_base_path {
            if !base.is_empty() {
                parts.push(base.trim_end_matches('/'));
            }
        }

        // Add method path (e.g., "/getMenuTree")
        parts.push(method_path);

        // Join and ensure starts with /
        let result = parts.join("");
        if result.starts_with('/') {
            result
        } else {
            format!("/{}", result)
        }
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
        let get_mapping = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&get_mapping),
            Some("GET".to_string())
        );

        let post_mapping = JavaAnnotation {
            name: "PostMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&post_mapping),
            Some("POST".to_string())
        );

        let put_mapping = JavaAnnotation {
            name: "PutMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&put_mapping),
            Some("PUT".to_string())
        );

        let delete_mapping = JavaAnnotation {
            name: "DeleteMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&delete_mapping),
            Some("DELETE".to_string())
        );

        let patch_mapping = JavaAnnotation {
            name: "PatchMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&patch_mapping),
            Some("PATCH".to_string())
        );

        let other = JavaAnnotation {
            name: "Other".to_string(),
            arguments: vec![],
        };
        assert_eq!(EndpointExtractor::get_http_method(&other), None);

        // Test @RequestMapping with explicit POST method
        let request_mapping_post = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("method".to_string()),
                value: "RequestMethod.POST".to_string(),
            }],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&request_mapping_post),
            Some("POST".to_string())
        );

        // Test @RequestMapping without method (defaults to GET)
        let request_mapping_default = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_method(&request_mapping_default),
            Some("GET".to_string())
        );
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

    #[test]
    fn test_extract_paths_from_annotation_single_path() {
        let annotation = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: None,
                value: "\"/api/users\"".to_string(),
            }],
        };

        let paths = EndpointExtractor::extract_paths_from_annotation_with_context(&annotation, None);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "/api/users");
    }

    #[test]
    fn test_extract_paths_from_annotation_multiple_paths() {
        let annotation = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("value".to_string()),
                value: "[\"/user\", \"/users\", \"/customer\"]".to_string(),
            }],
        };

        let paths = EndpointExtractor::extract_paths_from_annotation_with_context(&annotation, None);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "/user");
        assert_eq!(paths[1], "/users");
        assert_eq!(paths[2], "/customer");
    }

    #[test]
    fn test_extract_paths_with_curly_braces() {
        // Test Java array syntax with curly braces: {"/abc", "/xyz"}
        let annotation = JavaAnnotation {
            name: "PostMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("value".to_string()),
                value: "{\"/abc\", \"/xyz\"}".to_string(),
            }],
        };

        let paths = EndpointExtractor::extract_paths_from_annotation_with_context(&annotation, None);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "/abc");
        assert_eq!(paths[1], "/xyz");
    }

    #[test]
    fn test_extract_paths_curly_braces_without_quotes() {
        // Test: {/abc, /xyz} (without quotes around braces)
        let annotation = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("value".to_string()),
                value: "{\"/advertisementms\", \"/retail-pilot-advertisementms\"}".to_string(),
            }],
        };

        let paths = EndpointExtractor::extract_paths_from_annotation_with_context(&annotation, None);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "/advertisementms");
        assert_eq!(paths[1], "/retail-pilot-advertisementms");
    }

    #[test]
    fn test_extract_paths_without_leading_slash() {
        let annotation = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("value".to_string()),
                value: "[\"api/v1\", \"api/v2\"]".to_string(),
            }],
        };

        let paths = EndpointExtractor::extract_paths_from_annotation_with_context(&annotation, None);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "/api/v1");
        assert_eq!(paths[1], "/api/v2");
    }

    #[test]
    fn test_extract_base_paths_from_class_multiple() {
        let class_annotations = r#"[
            {
                "name": "RestController",
                "arguments": []
            },
            {
                "name": "RequestMapping",
                "arguments": [
                    {
                        "name": "value",
                        "value": "[\"/advertisementms\", \"retail/advertisementms\", \"retail_pilot/advertisementms\"]"
                    }
                ]
            }
        ]"#;

        let paths = EndpointExtractor::extract_base_paths_from_class_with_context(class_annotations, None);
        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "/advertisementms");
        assert_eq!(paths[1], "/retail/advertisementms");
        assert_eq!(paths[2], "/retail_pilot/advertisementms");
    }

    #[test]
    fn test_cartesian_product_endpoints() {
        use crate::analysis::languages::java::constant_resolver::ConstantResolver;

        let method_annotations = r#"[
            {
                "name": "RequestMapping",
                "arguments": [
                    {
                        "name": "value",
                        "value": "[\"/getMenuTree\", \"/menu\"]"
                    },
                    {
                        "name": "method",
                        "value": "RequestMethod.POST"
                    }
                ]
            }
        ]"#;

        let class_annotations = r#"[
            {
                "name": "RestController",
                "arguments": []
            },
            {
                "name": "RequestMapping",
                "arguments": [
                    {
                        "name": "value",
                        "value": "[\"/api/v1\", \"/api/v2\"]"
                    }
                ]
            }
        ]"#;

        let constant_resolver = ConstantResolver::new();
        let context = ExtractionContext {
            constant_resolver: &constant_resolver,
            all_definitions: &[],
            context_path: None,
        };

        let endpoints = EndpointExtractor::extract_endpoints_with_context(
            method_annotations,
            "com.example.Controller.getMenuTree",
            "Controller.java",
            10,
            20,
            Some(class_annotations),
            &context,
        );

        // Should create 2 class paths × 2 method paths = 4 endpoints
        assert_eq!(endpoints.len(), 4);

        // Verify all combinations
        assert!(endpoints.iter().any(|e| e.full_path == "/api/v1/getMenuTree"));
        assert!(endpoints.iter().any(|e| e.full_path == "/api/v1/menu"));
        assert!(endpoints.iter().any(|e| e.full_path == "/api/v2/getMenuTree"));
        assert!(endpoints.iter().any(|e| e.full_path == "/api/v2/menu"));

        // All should be POST
        assert!(endpoints.iter().all(|e| e.http_method == "POST"));
    }

    #[test]
    fn test_get_http_methods_specific_mappings() {
        // Test specific mapping annotations
        let get_mapping = JavaAnnotation {
            name: "GetMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&get_mapping),
            vec!["GET".to_string()]
        );

        let post_mapping = JavaAnnotation {
            name: "PostMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&post_mapping),
            vec!["POST".to_string()]
        );

        let put_mapping = JavaAnnotation {
            name: "PutMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&put_mapping),
            vec!["PUT".to_string()]
        );

        let delete_mapping = JavaAnnotation {
            name: "DeleteMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&delete_mapping),
            vec!["DELETE".to_string()]
        );

        let patch_mapping = JavaAnnotation {
            name: "PatchMapping".to_string(),
            arguments: vec![],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&patch_mapping),
            vec!["PATCH".to_string()]
        );
    }

    #[test]
    fn test_get_http_methods_request_mapping_without_method() {
        // Test @RequestMapping without method argument - should return all HTTP methods
        let request_mapping = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![],
        };

        let methods = EndpointExtractor::get_http_methods(&request_mapping);
        assert_eq!(methods.len(), 5);
        assert!(methods.contains(&"GET".to_string()));
        assert!(methods.contains(&"POST".to_string()));
        assert!(methods.contains(&"PUT".to_string()));
        assert!(methods.contains(&"DELETE".to_string()));
        assert!(methods.contains(&"PATCH".to_string()));
    }

    #[test]
    fn test_get_http_methods_request_mapping_with_single_method() {
        // Test @RequestMapping with explicit POST method
        let request_mapping_post = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("method".to_string()),
                value: "RequestMethod.POST".to_string(),
            }],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&request_mapping_post),
            vec!["POST".to_string()]
        );

        // Test @RequestMapping with GET method
        let request_mapping_get = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("method".to_string()),
                value: "RequestMethod.GET".to_string(),
            }],
        };
        assert_eq!(
            EndpointExtractor::get_http_methods(&request_mapping_get),
            vec!["GET".to_string()]
        );
    }

    #[test]
    fn test_get_http_methods_request_mapping_with_multiple_methods() {
        // Test @RequestMapping with multiple methods: {RequestMethod.GET, RequestMethod.POST}
        let request_mapping_multiple = JavaAnnotation {
            name: "RequestMapping".to_string(),
            arguments: vec![parser_core::java::types::JavaAnnotationArgument {
                name: Some("method".to_string()),
                value: "{RequestMethod.GET, RequestMethod.POST}".to_string(),
            }],
        };

        let methods = EndpointExtractor::get_http_methods(&request_mapping_multiple);
        assert_eq!(methods.len(), 2);
        assert!(methods.contains(&"GET".to_string()));
        assert!(methods.contains(&"POST".to_string()));
    }

    #[test]
    fn test_get_http_methods_non_endpoint_annotation() {
        // Test non-endpoint annotation
        let other = JavaAnnotation {
            name: "Autowired".to_string(),
            arguments: vec![],
        };
        assert_eq!(EndpointExtractor::get_http_methods(&other), Vec::<String>::new());
    }
}
