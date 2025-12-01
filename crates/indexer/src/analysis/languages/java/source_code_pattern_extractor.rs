use crate::analysis::types::ServiceCallNode;
use regex::Regex;

/// Extracts HTTP service calls by analyzing source code patterns
/// This detector works by reading method bodies and identifying HTTP client usage patterns
pub struct SourceCodePatternExtractor {
    // Compiled regex patterns for performance
    webclient_pattern: Regex,
    webclient_create_pattern: Regex,
    resttemplate_patterns: Vec<(Regex, &'static str)>,
    java11_http_pattern: Regex,
    java11_request_pattern: Regex,
    okhttp_url_pattern: Regex,
    okhttp_method_pattern: Regex,
    apache_http_patterns: Vec<(Regex, &'static str)>,
}

impl SourceCodePatternExtractor {
    pub fn new() -> Self {
        // WebClient patterns
        let webclient_pattern = Regex::new(
            r#"(?s)\.(?P<method>get|post|put|delete|patch)\s*\(\s*\).*?\.uri\s*\(\s*"(?P<url>[^"]+)""#
        ).expect("Failed to compile webclient_pattern");

        let webclient_create_pattern = Regex::new(
            r#"WebClient\.(?:create|builder)\s*\(\s*\)?\s*(?:\.baseUrl\s*)?\(\s*"(?P<base_url>[^"]+)""#
        ).expect("Failed to compile webclient_create_pattern");

        // RestTemplate patterns
        let resttemplate_patterns = vec![
            (Regex::new(r#"\.getForObject\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "GET"),
            (Regex::new(r#"\.getForEntity\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "GET"),
            (Regex::new(r#"\.postForObject\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "POST"),
            (Regex::new(r#"\.postForEntity\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "POST"),
            (Regex::new(r#"\.postForLocation\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "POST"),
            (Regex::new(r#"\.put\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "PUT"),
            (Regex::new(r#"\.delete\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "DELETE"),
            (Regex::new(r#"\.patchForObject\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "PATCH"),
            (Regex::new(r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.GET"#).unwrap(), "GET"),
            (Regex::new(r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.POST"#).unwrap(), "POST"),
            (Regex::new(r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.PUT"#).unwrap(), "PUT"),
            (Regex::new(r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.DELETE"#).unwrap(), "DELETE"),
        ];

        // Java 11+ HttpClient patterns
        let java11_http_pattern = Regex::new(
            r#"HttpClient\.(?:newHttpClient|newBuilder)"#
        ).expect("Failed to compile java11_http_pattern");

        let java11_request_pattern = Regex::new(
            r#"HttpRequest\.newBuilder\s*\(\s*\)\s*\.uri\s*\(\s*URI\.create\s*\(\s*"(?P<url>[^"]+)""#
        ).expect("Failed to compile java11_request_pattern");

        // OkHttp patterns
        let okhttp_url_pattern = Regex::new(
            r#"\.url\s*\(\s*"(?P<url>[^"]+)""#
        ).expect("Failed to compile okhttp_url_pattern");

        let okhttp_method_pattern = Regex::new(
            r#"\.(get|post|put|delete|patch)\s*\("#
        ).expect("Failed to compile okhttp_method_pattern");

        // Apache HttpClient patterns
        let apache_http_patterns = vec![
            (Regex::new(r#"new\s+HttpGet\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "GET"),
            (Regex::new(r#"new\s+HttpPost\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "POST"),
            (Regex::new(r#"new\s+HttpPut\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "PUT"),
            (Regex::new(r#"new\s+HttpDelete\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "DELETE"),
            (Regex::new(r#"new\s+HttpPatch\s*\(\s*"(?P<url>[^"]+)""#).unwrap(), "PATCH"),
        ];

        Self {
            webclient_pattern,
            webclient_create_pattern,
            resttemplate_patterns,
            java11_http_pattern,
            java11_request_pattern,
            okhttp_url_pattern,
            okhttp_method_pattern,
            apache_http_patterns,
        }
    }

    /// Detect HTTP service calls from source code based on imported HTTP client libraries
    /// This is called when we detect relevant imports (WebClient, RestTemplate, etc.)
    pub fn extract_from_source_code(
        &self,
        source_code: &str,
        import_types: &[&str], // e.g., ["WebClient", "RestTemplate"]
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        file_start_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut service_calls = Vec::new();

        // Try each detector based on imports
        for import_type in import_types {
            match *import_type {
                "WebClient" => {
                    service_calls.extend(self.extract_webclient_calls(
                        source_code,
                        class_fqn,
                        class_name,
                        file_path,
                        file_start_line,
                    ));
                }
                "RestTemplate" => {
                    service_calls.extend(self.extract_resttemplate_calls(
                        source_code,
                        class_fqn,
                        class_name,
                        file_path,
                        file_start_line,
                    ));
                }
                "HttpClient" if import_type.contains("java.net.http") => {
                    service_calls.extend(self.extract_java11_httpclient_calls(
                        source_code,
                        class_fqn,
                        class_name,
                        file_path,
                        file_start_line,
                    ));
                }
                "OkHttpClient" => {
                    service_calls.extend(self.extract_okhttp_calls(
                        source_code,
                        class_fqn,
                        class_name,
                        file_path,
                        file_start_line,
                    ));
                }
                "HttpClient" | "CloseableHttpClient" if import_type.contains("apache") => {
                    service_calls.extend(self.extract_apache_httpclient_calls(
                        source_code,
                        class_fqn,
                        class_name,
                        file_path,
                        file_start_line,
                    ));
                }
                _ => {}
            }
        }

        service_calls
    }

    /// Extract WebClient HTTP calls
    /// Pattern: webClient.get().uri("http://...") or WebClient.create("http://...")
    fn extract_webclient_calls(
        &self,
        source_code: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        base_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut calls = Vec::new();

        // Pattern 1: webClient.get().uri("...")
        for cap in self.webclient_pattern.captures_iter(source_code) {
            let http_method = cap.name("method")
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_else(|| "GET".to_string());
            let url = cap.name("url")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let line_number = self.calculate_line_number(source_code, cap.get(0).unwrap().start(), base_line);
            let path = self.extract_path_from_url(&url);

            log::info!(
                "[SOURCE_PATTERN] Detected WebClient call: {} {} at {}:{}",
                http_method,
                url,
                file_path,
                line_number
            );

            calls.push(ServiceCallNode::new(
                "WebClient".to_string(),
                class_name.to_string(),
                None, // original_service_url
                url.clone(),
                http_method,
                None, // original_path
                path,
                None, // original_full_path
                url,
                class_name.to_string(),
                class_fqn.to_string(),
                "".to_string(), // Method name not easily extractable from regex
                class_fqn.to_string(),
                file_path.to_string(),
                line_number,
                line_number,
            ));
        }

        // Pattern 2: WebClient.create("baseUrl") or .baseUrl("...")
        for cap in self.webclient_create_pattern.captures_iter(source_code) {
            if let Some(base_url) = cap.name("base_url") {
                let url = base_url.as_str().to_string();
                let line_number = self.calculate_line_number(source_code, cap.get(0).unwrap().start(), base_line);
                let path = self.extract_path_from_url(&url);

                log::info!(
                    "[SOURCE_PATTERN] Detected WebClient.create: {} at {}:{}",
                    url,
                    file_path,
                    line_number
                );

                calls.push(ServiceCallNode::new(
                    "WebClient".to_string(),
                    class_name.to_string(),
                    None, // original_service_url
                    url.clone(),
                    "HTTP".to_string(),
                    None, // original_path
                    path,
                    None, // original_full_path
                    url,
                    class_name.to_string(),
                    class_fqn.to_string(),
                    "".to_string(),
                    class_fqn.to_string(),
                    file_path.to_string(),
                    line_number,
                    line_number,
                ));
            }
        }

        calls
    }

    /// Extract RestTemplate HTTP calls
    fn extract_resttemplate_calls(
        &self,
        source_code: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        base_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut calls = Vec::new();

        for (pattern, http_method) in &self.resttemplate_patterns {
            for cap in pattern.captures_iter(source_code) {
                if let Some(url_match) = cap.name("url") {
                    let url = url_match.as_str().to_string();
                    let line_number = self.calculate_line_number(source_code, cap.get(0).unwrap().start(), base_line);
                    let path = self.extract_path_from_url(&url);

                    log::info!(
                        "[SOURCE_PATTERN] Detected RestTemplate call: {} {} at {}:{}",
                        http_method,
                        url,
                        file_path,
                        line_number
                    );

                    calls.push(ServiceCallNode::new(
                        "RestTemplate".to_string(),
                        class_name.to_string(),
                        None, // original_service_url
                        url.clone(),
                        http_method.to_string(),
                        None, // original_path
                        path,
                        None, // original_full_path
                        url,
                        class_name.to_string(),
                        class_fqn.to_string(),
                        "".to_string(),
                        class_fqn.to_string(),
                        file_path.to_string(),
                        line_number,
                        line_number,
                    ));
                }
            }
        }

        calls
    }

    /// Extract Java 11+ HttpClient calls
    fn extract_java11_httpclient_calls(
        &self,
        source_code: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        base_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut calls = Vec::new();

        // Check if HttpClient is actually used
        if !self.java11_http_pattern.is_match(source_code) {
            return calls;
        }

        // Extract HttpRequest.newBuilder().uri(URI.create("..."))
        for cap in self.java11_request_pattern.captures_iter(source_code) {
            if let Some(url_match) = cap.name("url") {
                let url = url_match.as_str().to_string();
                let line_number = self.calculate_line_number(source_code, cap.get(0).unwrap().start(), base_line);
                let path = self.extract_path_from_url(&url);

                log::info!(
                    "[SOURCE_PATTERN] Detected Java 11 HttpClient call: {} at {}:{}",
                    url,
                    file_path,
                    line_number
                );

                calls.push(ServiceCallNode::new(
                    "Java11HttpClient".to_string(),
                    class_name.to_string(),
                    None, // original_service_url
                    url.clone(),
                    "HTTP".to_string(), // Method not easily determinable from pattern
                    None, // original_path
                    path,
                    None, // original_full_path
                    url,
                    class_name.to_string(),
                    class_fqn.to_string(),
                    "".to_string(),
                    class_fqn.to_string(),
                    file_path.to_string(),
                    line_number,
                    line_number,
                ));
            }
        }

        calls
    }

    /// Extract OkHttp calls
    fn extract_okhttp_calls(
        &self,
        source_code: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        base_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut calls = Vec::new();

        for cap in self.okhttp_url_pattern.captures_iter(source_code) {
            if let Some(url_match) = cap.name("url") {
                let url = url_match.as_str().to_string();
                let match_start = cap.get(0).unwrap().start();
                let line_number = self.calculate_line_number(source_code, match_start, base_line);
                let path = self.extract_path_from_url(&url);

                // Try to find HTTP method near the URL
                let http_method = self.find_okhttp_method(source_code, match_start);

                log::info!(
                    "[SOURCE_PATTERN] Detected OkHttp call: {} {} at {}:{}",
                    http_method,
                    url,
                    file_path,
                    line_number
                );

                calls.push(ServiceCallNode::new(
                    "OkHttp".to_string(),
                    class_name.to_string(),
                    None, // original_service_url
                    url.clone(),
                    http_method,
                    None, // original_path
                    path,
                    None, // original_full_path
                    url,
                    class_name.to_string(),
                    class_fqn.to_string(),
                    "".to_string(),
                    class_fqn.to_string(),
                    file_path.to_string(),
                    line_number,
                    line_number,
                ));
            }
        }

        calls
    }

    /// Extract Apache HttpClient calls
    fn extract_apache_httpclient_calls(
        &self,
        source_code: &str,
        class_fqn: &str,
        class_name: &str,
        file_path: &str,
        base_line: i32,
    ) -> Vec<ServiceCallNode> {
        let mut calls = Vec::new();

        for (pattern, http_method) in &self.apache_http_patterns {
            for cap in pattern.captures_iter(source_code) {
                if let Some(url_match) = cap.name("url") {
                    let url = url_match.as_str().to_string();
                    let line_number = self.calculate_line_number(source_code, cap.get(0).unwrap().start(), base_line);
                    let path = self.extract_path_from_url(&url);

                    log::info!(
                        "[SOURCE_PATTERN] Detected Apache HttpClient call: {} {} at {}:{}",
                        http_method,
                        url,
                        file_path,
                        line_number
                    );

                    calls.push(ServiceCallNode::new(
                        "HttpClient".to_string(),
                        class_name.to_string(),
                        None, // original_service_url
                        url.clone(),
                        http_method.to_string(),
                        None, // original_path
                        path,
                        None, // original_full_path
                        url,
                        class_name.to_string(),
                        class_fqn.to_string(),
                        "".to_string(),
                        class_fqn.to_string(),
                        file_path.to_string(),
                        line_number,
                        line_number,
                    ));
                }
            }
        }

        calls
    }

    /// Calculate line number from byte offset
    fn calculate_line_number(&self, source: &str, byte_offset: usize, base_line: i32) -> i32 {
        let line_offset = source[..byte_offset].matches('\n').count() as i32;
        base_line + line_offset
    }

    /// Extract path from full URL
    fn extract_path_from_url(&self, url: &str) -> String {
        let url = url.trim();

        // If it starts with http:// or https://, extract path
        if let Some(pos) = url.find("://") {
            let after_protocol = &url[pos + 3..];
            if let Some(path_start) = after_protocol.find('/') {
                return after_protocol[path_start..].to_string();
            }
            return "/".to_string();
        }

        // If it's a relative path or template
        if url.starts_with('/') || url.starts_with("${") {
            return url.to_string();
        }

        // Otherwise return as-is
        url.to_string()
    }

    /// Find HTTP method for OkHttp by looking around the .url() call
    fn find_okhttp_method(&self, source: &str, match_start: usize) -> String {
        // Look in a 200-char window around the match
        let context_start = match_start.saturating_sub(200);
        let context_end = source.len().min(match_start + 200);
        let context = &source[context_start..context_end];

        if let Some(cap) = self.okhttp_method_pattern.captures(context) {
            if let Some(method) = cap.get(1) {
                return method.as_str().to_uppercase();
            }
        }

        "GET".to_string()
    }
}

impl Default for SourceCodePatternExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_webclient_get_uri() {
        let extractor = SourceCodePatternExtractor::new();
        let source = r#"
            InventoryResponse[] responses = webClient.get()
                .uri("http://localhost:8186/api/v1/inventory")
                .retrieve()
                .bodyToMono(InventoryResponse[].class)
                .block();
        "#;

        let calls = extractor.extract_webclient_calls(
            source,
            "com.example.OrderService",
            "OrderService",
            "OrderService.java",
            1,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].service_type, "WebClient");
        assert_eq!(calls[0].http_method, "GET");
        assert_eq!(calls[0].service_url, "http://localhost:8186/api/v1/inventory");
        assert_eq!(calls[0].path, "/api/v1/inventory");
    }

    #[test]
    fn test_extract_resttemplate_get() {
        let extractor = SourceCodePatternExtractor::new();
        let source = r#"
            User user = restTemplate.getForObject("http://localhost:8080/users/1", User.class);
        "#;

        let calls = extractor.extract_resttemplate_calls(
            source,
            "com.example.UserService",
            "UserService",
            "UserService.java",
            1,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].service_type, "RestTemplate");
        assert_eq!(calls[0].http_method, "GET");
        assert_eq!(calls[0].service_url, "http://localhost:8080/users/1");
    }

    #[test]
    fn test_extract_apache_httpclient() {
        let extractor = SourceCodePatternExtractor::new();
        let source = r#"
            HttpGet request = new HttpGet("http://localhost:8080/api/data");
            CloseableHttpResponse response = client.execute(request);
        "#;

        let calls = extractor.extract_apache_httpclient_calls(
            source,
            "com.example.DataService",
            "DataService",
            "DataService.java",
            1,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].service_type, "HttpClient");
        assert_eq!(calls[0].http_method, "GET");
        assert_eq!(calls[0].service_url, "http://localhost:8080/api/data");
    }

    #[test]
    fn test_webclient_post() {
        let extractor = SourceCodePatternExtractor::new();
        let source = r#"
            return webClient.post()
                .uri("http://api.example.com/orders")
                .bodyValue(order)
                .retrieve()
                .bodyToMono(OrderResponse.class);
        "#;

        let calls = extractor.extract_webclient_calls(
            source,
            "com.example.OrderService",
            "OrderService",
            "OrderService.java",
            10,
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].http_method, "POST");
        assert_eq!(calls[0].service_url, "http://api.example.com/orders");
    }
}
