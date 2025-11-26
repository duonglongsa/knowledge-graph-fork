// Legacy code warning suppression - this file contains legacy functions that will be removed
// once the migration to ServiceCallNode-based queries is fully tested
#![allow(dead_code)]

use crate::AppState;
use crate::contract::{EmptyRequest, EndpointConfigTypes};
use crate::decode_url_param;
use crate::define_endpoint;
use crate::endpoints::shared::StatusResponse;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use database::querying::{QueryLibrary, QueryResult, QueryingService, service::DatabaseQueryingService};
use event_bus::types::project_info::{TSProjectInfo, to_ts_project_info};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tracing::{info, warn, error};
use ts_rs::TS;

use super::shared::{JavaAnnotation as JavaAnnotationInfo, JavaAnnotationArgument};

/// Represents a Java external service call (e.g., FeignClient, RestTemplate, WebClient)
#[derive(Serialize, Deserialize, TS, Debug, Clone)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaServiceCall {
    pub service_type: String,
    pub service_name: String,
    pub service_url: String,
    pub http_method: String,
    pub path: String,
    pub full_path: String,
    pub class_name: String,
    pub class_fqn: String,
    pub method_name: String,
    pub method_fqn: String,
    pub file_path: String,
    pub line_number: i64,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaServiceCallsPathRequest {
    pub workspace_folder_path: String,
    pub project_path: String,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaServiceCallsQueryRequest {
    pub limit: Option<i32>,
}

#[derive(Serialize, Deserialize, TS, Default, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaServiceCallsSuccessResponse {
    pub service_calls: Vec<JavaServiceCall>,
    pub project_info: TSProjectInfo,
}

#[derive(Serialize, Deserialize, TS, Default, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaServiceCallsResponses {
    #[serde(rename = "200")]
    pub ok: Option<JavaServiceCallsSuccessResponse>,
    #[serde(rename = "404")]
    pub not_found: Option<StatusResponse>,
    #[serde(rename = "400")]
    pub bad_request: Option<StatusResponse>,
    #[serde(rename = "500")]
    pub internal_server_error: Option<StatusResponse>,
}

pub struct JavaServiceCallsEndpointConfig;

impl EndpointConfigTypes for JavaServiceCallsEndpointConfig {
    type PathRequest = JavaServiceCallsPathRequest;
    type BodyRequest = EmptyRequest;
    type QueryRequest = JavaServiceCallsQueryRequest;
    type Response = JavaServiceCallsSuccessResponse;
}

define_endpoint! {
    JavaServiceCallsEndpoint,
    JavaServiceCallsEndpointDef,
    Get,
    "/graph/java-service-calls/{workspace_folder_path}/{project_path}",
    ts_path_type = "\"/api/graph/java-service-calls/{workspace_folder_path}/{project_path}\"",
    config = JavaServiceCallsEndpointConfig,
    export_to = "../../../packages/gkg/src/api.ts"
}

impl JavaServiceCallsEndpoint {
    pub fn create_success_response(
        service_calls: Vec<JavaServiceCall>,
        project_info: TSProjectInfo,
    ) -> JavaServiceCallsSuccessResponse {
        JavaServiceCallsSuccessResponse {
            service_calls,
            project_info,
        }
    }

    pub fn create_error_response(status: String) -> StatusResponse {
        StatusResponse { status }
    }
}

pub async fn java_service_calls_handler(
    State(state): State<AppState>,
    Path(path_params): Path<JavaServiceCallsPathRequest>,
    Query(query_params): Query<JavaServiceCallsQueryRequest>,
) -> impl IntoResponse {
    let input_project_path = decode_url_param!(
        &path_params.project_path,
        "project_path",
        JavaServiceCallsEndpoint::create_error_response
    );
    let input_workspace_folder_path = decode_url_param!(
        &path_params.workspace_folder_path,
        "workspace_folder_path",
        JavaServiceCallsEndpoint::create_error_response
    );

    let limit = query_params.limit.unwrap_or(1000);

    if input_project_path.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(JavaServiceCallsEndpoint::create_error_response(
                "empty_project_path".to_string(),
            )),
        )
            .into_response();
    }

    info!(
        "Received java service calls request {workspace_folder_path} {project_path} limit={limit}",
        workspace_folder_path = input_workspace_folder_path,
        project_path = input_project_path,
        limit = limit
    );

    let project_info = match state
        .workspace_manager
        .get_project_info(&input_workspace_folder_path, &input_project_path)
    {
        Some(info) => info,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(JavaServiceCallsEndpoint::create_error_response(
                    "project_not_found".to_string(),
                )),
            )
                .into_response();
        }
    };

    let query_service = DatabaseQueryingService::new(Arc::clone(&state.database));

    info!(
        "Executing service calls query for project {} and workspace folder {}, limit={}",
        project_info.project_path, input_workspace_folder_path, limit
    );

    // Single query to ServiceCallNode table - replaces 6 complex queries
    let service_calls_query = QueryLibrary::get_service_calls_query();
    let mut query_params_map = serde_json::Map::new();
    query_params_map.insert("limit".to_string(), serde_json::Value::Number(limit.into()));

    let all_service_calls = match query_service.execute_query(
        project_info.database_path.clone(),
        service_calls_query.query,
        query_params_map,
    ) {
        Ok(mut query_result) => {
            // Direct conversion from database results to JavaServiceCall
            convert_service_call_results(&mut query_result).unwrap_or_else(|e| {
                warn!("Failed to convert service call results: {}", e);
                Vec::new()
            })
        }
        Err(e) => {
            error!("Failed to execute service calls query: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JavaServiceCallsEndpoint::create_error_response(
                    "query_execution_failed".to_string(),
                )),
            )
                .into_response();
        }
    };

    info!(
        "Successfully retrieved {} service calls for project {}",
        all_service_calls.len(),
        project_info.project_path
    );

    (
        StatusCode::OK,
        Json(JavaServiceCallsEndpoint::create_success_response(
            all_service_calls,
            to_ts_project_info(&project_info),
        )),
    )
        .into_response()
}

/// Convert service call query results directly from ServiceCallNode table
/// This is a simple 1:1 mapping from database to API response
fn convert_service_call_results(
    query_result: &mut Box<dyn QueryResult>,
) -> Result<Vec<JavaServiceCall>, Box<dyn std::error::Error>> {
    let mut service_calls = Vec::new();

    while let Some(row) = query_result.next() {
        // Extract all fields from the ServiceCallNode query result
        let _id = row.get_int_value(0)? as u32; // id field (not used in response)
        let service_type = row.get_string_value(1)?;
        let service_name = row.get_string_value(2)?;
        let service_url = row.get_string_value(3)?;
        let http_method = row.get_string_value(4)?;
        let path = row.get_string_value(5)?;
        let full_path = row.get_string_value(6)?;
        let class_name = row.get_string_value(7)?;
        let class_fqn = row.get_string_value(8)?;
        let method_name = row.get_string_value(9)?;
        let method_fqn = row.get_string_value(10)?;
        let file_path = row.get_string_value(11)?;
        let start_line = row.get_int_value(12)?;
        let _end_line = row.get_int_value(13)?; // end_line (not used in response)

        // Skip class-level entries (only include actual method calls)
        // Class-level @FeignClient annotations have empty method_name
        if method_name.is_empty() {
            continue;
        }

        let service_call = JavaServiceCall {
            service_type,
            service_name,
            service_url,
            http_method,
            path,
            full_path,
            class_name,
            class_fqn,
            method_name,
            method_fqn,
            file_path,
            line_number: start_line,
        };
        service_calls.push(service_call);
    }

    Ok(service_calls)
}

// Legacy conversion functions - kept for reference but no longer used
// These can be removed in a future cleanup
#[allow(dead_code)]
/// Convert FeignClient query results to service calls
fn convert_feign_client_results(
    query_result: &mut Box<dyn QueryResult>,
) -> Result<Vec<JavaServiceCall>, Box<dyn std::error::Error>> {
    let mut service_calls = Vec::new();

    while let Some(row) = query_result.next() {
        let _service_type = row.get_string_value(0)?; // "FeignClient"
        let class_name = row.get_string_value(1)?;
        let class_fqn = row.get_string_value(2)?;
        let file_path = row.get_string_value(3)?;
        let class_annotations_json = row.get_string_value(4)?;
        let method_name = row.get_string_value(5)?;
        let method_fqn = row.get_string_value(6)?;
        let method_start_line = row.get_int_value(7)?;
        let method_annotations_json = row.get_string_value(8)?;

        // Parse interface annotations to get FeignClient info
        let (service_name, service_url) = extract_feign_client_info(&class_annotations_json);

        // Parse method annotations to get HTTP method and path
        let method_endpoints = extract_method_endpoints(&method_annotations_json);

        for (http_method, method_path) in method_endpoints {
            let full_path = combine_paths(&service_url, &method_path);

            service_calls.push(JavaServiceCall {
                service_type: "FeignClient".to_string(),
                service_name: service_name.clone(),
                service_url: service_url.clone(),
                http_method,
                path: method_path,
                full_path,
                class_name: class_name.clone(),
                class_fqn: class_fqn.clone(),
                method_name: method_name.clone(),
                method_fqn: method_fqn.clone(),
                file_path: file_path.clone(),
                line_number: method_start_line,
            });
        }
    }

    Ok(service_calls)
}

/// Convert RestTemplate query results to service calls
fn convert_rest_template_results(
    query_result: &mut Box<dyn QueryResult>,
) -> Result<Vec<JavaServiceCall>, Box<dyn std::error::Error>> {
    let mut service_calls = Vec::new();

    while let Some(row) = query_result.next() {
        let _service_type = row.get_string_value(0)?; // "RestTemplate"
        let class_name = row.get_string_value(1)?;
        let class_fqn = row.get_string_value(2)?;
        let file_path = row.get_string_value(3)?;
        let _class_annotations_json = row.get_string_value(4)?;
        let method_name = row.get_string_value(5)?;
        let method_fqn = row.get_string_value(6)?;
        let method_start_line = row.get_int_value(7)?;
        let _method_annotations_json = row.get_string_value(8)?;

        // Try to read method source and extract HTTP calls
        if let Some(method_source) = read_method_source(&file_path, method_start_line, &class_name) {
            let extracted_calls = extract_resttemplate_calls(&method_source, method_start_line);

            if !extracted_calls.is_empty() {
                for call in extracted_calls {
                    let path = extract_path_from_url(&call.url);
                    service_calls.push(JavaServiceCall {
                        service_type: "RestTemplate".to_string(),
                        service_name: class_name.clone(),
                        service_url: call.url.clone(),
                        http_method: call.http_method,
                        path: path.clone(),
                        full_path: call.url,
                        class_name: class_name.clone(),
                        class_fqn: class_fqn.clone(),
                        method_name: method_name.clone(),
                        method_fqn: method_fqn.clone(),
                        file_path: file_path.clone(),
                        line_number: call.line_number,
                    });
                }
                continue;
            }
        }

        // Fallback: add the method as a potential HTTP call point
        service_calls.push(JavaServiceCall {
            service_type: "RestTemplate".to_string(),
            service_name: class_name.clone(),
            service_url: String::new(),
            http_method: "HTTP".to_string(),
            path: String::new(),
            full_path: String::new(),
            class_name: class_name.clone(),
            class_fqn: class_fqn.clone(),
            method_name: method_name.clone(),
            method_fqn: method_fqn.clone(),
            file_path: file_path.clone(),
            line_number: method_start_line,
        });
    }

    Ok(service_calls)
}

/// Convert WebClient query results to service calls
fn convert_web_client_results(
    query_result: &mut Box<dyn QueryResult>,
) -> Result<Vec<JavaServiceCall>, Box<dyn std::error::Error>> {
    let mut service_calls = Vec::new();

    while let Some(row) = query_result.next() {
        let _service_type = row.get_string_value(0)?;
        let class_name = row.get_string_value(1)?;
        let class_fqn = row.get_string_value(2)?;
        let file_path = row.get_string_value(3)?;
        let _class_annotations_json = row.get_string_value(4)?;
        let method_name = row.get_string_value(5)?;
        let method_fqn = row.get_string_value(6)?;
        let method_start_line = row.get_int_value(7)?;
        let _method_annotations_json = row.get_string_value(8)?;

        // Try to read method source and extract HTTP calls
        if let Some(method_source) = read_method_source(&file_path, method_start_line, &class_name) {
            let extracted_calls = extract_webclient_calls(&method_source, method_start_line);

            if !extracted_calls.is_empty() {
                for call in extracted_calls {
                    let path = extract_path_from_url(&call.url);
                    service_calls.push(JavaServiceCall {
                        service_type: "WebClient".to_string(),
                        service_name: class_name.clone(),
                        service_url: call.url.clone(),
                        http_method: call.http_method,
                        path: path.clone(),
                        full_path: call.url,
                        class_name: class_name.clone(),
                        class_fqn: class_fqn.clone(),
                        method_name: method_name.clone(),
                        method_fqn: method_fqn.clone(),
                        file_path: file_path.clone(),
                        line_number: call.line_number,
                    });
                }
                continue;
            }
        }

        // Fallback: add the method as a potential HTTP call point
        service_calls.push(JavaServiceCall {
            service_type: "WebClient".to_string(),
            service_name: class_name.clone(),
            service_url: String::new(),
            http_method: "HTTP".to_string(),
            path: String::new(),
            full_path: String::new(),
            class_name: class_name.clone(),
            class_fqn: class_fqn.clone(),
            method_name: method_name.clone(),
            method_fqn: method_fqn.clone(),
            file_path: file_path.clone(),
            line_number: method_start_line,
        });
    }

    Ok(service_calls)
}

/// Generic converter for HTTP client query results (HttpClient, OkHttp, Retrofit, etc.)
fn convert_generic_http_client_results(
    query_result: &mut Box<dyn QueryResult>,
    service_type: &str,
) -> Result<Vec<JavaServiceCall>, Box<dyn std::error::Error>> {
    let mut service_calls = Vec::new();

    while let Some(row) = query_result.next() {
        let _service_type_from_query = row.get_string_value(0)?;
        let class_name = row.get_string_value(1)?;
        let class_fqn = row.get_string_value(2)?;
        let file_path = row.get_string_value(3)?;
        let _class_annotations_json = row.get_string_value(4)?;
        let method_name = row.get_string_value(5)?;
        let method_fqn = row.get_string_value(6)?;
        let method_start_line = row.get_int_value(7)?;
        let _method_annotations_json = row.get_string_value(8)?;

        // Try to read method source and extract HTTP calls based on service type
        if let Some(method_source) = read_method_source(&file_path, method_start_line, &class_name) {
            let extracted_calls = match service_type {
                "HttpClient" => extract_httpclient_calls(&method_source, method_start_line),
                "OkHttp" => extract_okhttp_calls(&method_source, method_start_line),
                _ => Vec::new(),
            };

            if !extracted_calls.is_empty() {
                for call in extracted_calls {
                    let path = extract_path_from_url(&call.url);
                    service_calls.push(JavaServiceCall {
                        service_type: service_type.to_string(),
                        service_name: class_name.clone(),
                        service_url: call.url.clone(),
                        http_method: call.http_method,
                        path: path.clone(),
                        full_path: call.url,
                        class_name: class_name.clone(),
                        class_fqn: class_fqn.clone(),
                        method_name: method_name.clone(),
                        method_fqn: method_fqn.clone(),
                        file_path: file_path.clone(),
                        line_number: call.line_number,
                    });
                }
                continue;
            }
        }

        // Fallback: add the method as a potential HTTP call point
        service_calls.push(JavaServiceCall {
            service_type: service_type.to_string(),
            service_name: class_name.clone(),
            service_url: String::new(),
            http_method: "HTTP".to_string(),
            path: String::new(),
            full_path: String::new(),
            class_name: class_name.clone(),
            class_fqn: class_fqn.clone(),
            method_name: method_name.clone(),
            method_fqn: method_fqn.clone(),
            file_path: file_path.clone(),
            line_number: method_start_line,
        });
    }

    Ok(service_calls)
}

/// Extract service name and URL from @FeignClient annotation
fn extract_feign_client_info(annotations_json: &str) -> (String, String) {
    if annotations_json.is_empty() {
        return (String::new(), String::new());
    }

    let annotations: Vec<JavaAnnotationInfo> = match serde_json::from_str(annotations_json) {
        Ok(a) => a,
        Err(_) => return (String::new(), String::new()),
    };

    for annotation in annotations {
        if annotation.name == "FeignClient" {
            let mut service_name = String::new();
            let mut service_url = String::new();

            for arg in &annotation.arguments {
                let value = clean_annotation_value(&arg.value);

                match arg.name.as_deref() {
                    Some("name") | Some("value") | None => {
                        if service_name.is_empty() {
                            service_name = value;
                        }
                    }
                    Some("url") => {
                        service_url = value;
                    }
                    Some("path") => {
                        if service_url.is_empty() {
                            service_url = value;
                        }
                    }
                    _ => {}
                }
            }

            return (service_name, service_url);
        }
    }

    (String::new(), String::new())
}

/// Extract HTTP method and path from method annotations
fn extract_method_endpoints(annotations_json: &str) -> Vec<(String, String)> {
    if annotations_json.is_empty() {
        return Vec::new();
    }

    let annotations: Vec<JavaAnnotationInfo> = match serde_json::from_str(annotations_json) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for annotation in annotations {
        let (http_method, path) = match annotation.name.as_str() {
            "GetMapping" => ("GET".to_string(), extract_path_from_arguments(&annotation.arguments)),
            "PostMapping" => ("POST".to_string(), extract_path_from_arguments(&annotation.arguments)),
            "PutMapping" => ("PUT".to_string(), extract_path_from_arguments(&annotation.arguments)),
            "DeleteMapping" => ("DELETE".to_string(), extract_path_from_arguments(&annotation.arguments)),
            "PatchMapping" => ("PATCH".to_string(), extract_path_from_arguments(&annotation.arguments)),
            "RequestMapping" => {
                let method = extract_http_method_from_arguments(&annotation.arguments);
                let path = extract_path_from_arguments(&annotation.arguments);
                (method, path)
            }
            _ => continue,
        };
        results.push((http_method, path));
    }

    results
}

/// Extract path value from annotation arguments
fn extract_path_from_arguments(arguments: &[JavaAnnotationArgument]) -> String {
    for arg in arguments {
        if arg.name.is_none() || arg.name.as_deref() == Some("value") || arg.name.as_deref() == Some("path") {
            let value = clean_annotation_value(&arg.value);
            if !value.is_empty() && !value.starts_with("RequestMethod") {
                return value;
            }
        }
    }
    String::new()
}

/// Extract HTTP method from @RequestMapping arguments
fn extract_http_method_from_arguments(arguments: &[JavaAnnotationArgument]) -> String {
    for arg in arguments {
        if arg.name.as_deref() == Some("method") {
            let value = arg.value.to_uppercase();
            if value.contains("GET") {
                return "GET".to_string();
            } else if value.contains("POST") {
                return "POST".to_string();
            } else if value.contains("PUT") {
                return "PUT".to_string();
            } else if value.contains("DELETE") {
                return "DELETE".to_string();
            } else if value.contains("PATCH") {
                return "PATCH".to_string();
            }
        }
    }
    "GET".to_string()
}

/// Clean annotation value by removing quotes and trimming
fn clean_annotation_value(value: &str) -> String {
    let value = value.trim();
    let value = value.trim_matches('"');
    let value = value.trim_matches('\'');
    value.to_string()
}

/// Combine service URL/path and method path
fn combine_paths(base_path: &str, method_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    let method = if method_path.starts_with('/') {
        method_path.to_string()
    } else if !method_path.is_empty() {
        format!("/{}", method_path)
    } else {
        String::new()
    };

    if base.is_empty() {
        if method.is_empty() {
            "/".to_string()
        } else {
            method
        }
    } else {
        format!("{}{}", base, method)
    }
}

/// Represents an HTTP call extracted from source code
#[derive(Debug, Clone)]
struct ExtractedHttpCall {
    http_method: String,
    url: String,
    line_number: i64,
}

/// Extract HTTP calls from a method's source code for WebClient
fn extract_webclient_calls(source: &str, method_start_line: i64) -> Vec<ExtractedHttpCall> {
    let mut calls = Vec::new();

    // Patterns for WebClient method chains
    // webClient.get().uri("...") or webClient.post().uri("...") etc.
    let webclient_pattern = Regex::new(
        r#"(?s)\.(?P<method>get|post|put|delete|patch)\s*\(\s*\).*?\.uri\s*\(\s*"(?P<url>[^"]+)""#
    ).ok();

    // Also handle WebClient.create("baseUrl").get().uri("...")
    let webclient_create_pattern = Regex::new(
        r#"WebClient\.create\s*\(\s*"(?P<base_url>[^"]+)""#
    ).ok();

    if let Some(pattern) = webclient_pattern {
        for cap in pattern.captures_iter(source) {
            let http_method = cap.name("method")
                .map(|m| m.as_str().to_uppercase())
                .unwrap_or_else(|| "GET".to_string());
            let url = cap.name("url")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            // Calculate approximate line number
            let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line_offset = source[..match_start].matches('\n').count() as i64;

            calls.push(ExtractedHttpCall {
                http_method,
                url,
                line_number: method_start_line + line_offset,
            });
        }
    }

    // If no method chain found, try to find base URL from WebClient.create()
    if calls.is_empty() {
        if let Some(create_pattern) = webclient_create_pattern {
            if let Some(cap) = create_pattern.captures(source) {
                let base_url = cap.name("base_url")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                if !base_url.is_empty() {
                    calls.push(ExtractedHttpCall {
                        http_method: "HTTP".to_string(),
                        url: base_url,
                        line_number: method_start_line,
                    });
                }
            }
        }
    }

    calls
}

/// Extract HTTP calls from a method's source code for RestTemplate
fn extract_resttemplate_calls(source: &str, method_start_line: i64) -> Vec<ExtractedHttpCall> {
    let mut calls = Vec::new();

    // Patterns for RestTemplate method calls
    // restTemplate.getForObject("url", ...) or restTemplate.postForEntity("url", ...) etc.
    let patterns = [
        (r#"\.getForObject\s*\(\s*"(?P<url>[^"]+)""#, "GET"),
        (r#"\.getForEntity\s*\(\s*"(?P<url>[^"]+)""#, "GET"),
        (r#"\.postForObject\s*\(\s*"(?P<url>[^"]+)""#, "POST"),
        (r#"\.postForEntity\s*\(\s*"(?P<url>[^"]+)""#, "POST"),
        (r#"\.postForLocation\s*\(\s*"(?P<url>[^"]+)""#, "POST"),
        (r#"\.put\s*\(\s*"(?P<url>[^"]+)""#, "PUT"),
        (r#"\.delete\s*\(\s*"(?P<url>[^"]+)""#, "DELETE"),
        (r#"\.patchForObject\s*\(\s*"(?P<url>[^"]+)""#, "PATCH"),
        // exchange() with HttpMethod
        (r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.GET"#, "GET"),
        (r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.POST"#, "POST"),
        (r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.PUT"#, "PUT"),
        (r#"\.exchange\s*\(\s*"(?P<url>[^"]+)"[^)]*HttpMethod\.DELETE"#, "DELETE"),
    ];

    for (pattern_str, method) in patterns {
        if let Ok(pattern) = Regex::new(pattern_str) {
            for cap in pattern.captures_iter(source) {
                let url = cap.name("url")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
                let line_offset = source[..match_start].matches('\n').count() as i64;

                calls.push(ExtractedHttpCall {
                    http_method: method.to_string(),
                    url,
                    line_number: method_start_line + line_offset,
                });
            }
        }
    }

    calls
}

/// Extract HTTP calls from source code for OkHttp
fn extract_okhttp_calls(source: &str, method_start_line: i64) -> Vec<ExtractedHttpCall> {
    let mut calls = Vec::new();

    // Pattern: new Request.Builder().url("...").get().build()
    let url_pattern = Regex::new(r#"\.url\s*\(\s*"(?P<url>[^"]+)""#).ok();
    let method_pattern = Regex::new(r#"\.(get|post|put|delete|patch)\s*\("#).ok();

    if let Some(url_pat) = url_pattern {
        for cap in url_pat.captures_iter(source) {
            let url = cap.name("url")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line_offset = source[..match_start].matches('\n').count() as i64;

            // Try to find HTTP method near the URL
            let http_method = if let Some(method_pat) = &method_pattern {
                let context = &source[match_start.saturating_sub(200)..source.len().min(match_start + 200)];
                method_pat.captures(context)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_uppercase())
                    .unwrap_or_else(|| "GET".to_string())
            } else {
                "GET".to_string()
            };

            calls.push(ExtractedHttpCall {
                http_method,
                url,
                line_number: method_start_line + line_offset,
            });
        }
    }

    calls
}

/// Extract HTTP calls from source code for Apache HttpClient
fn extract_httpclient_calls(source: &str, method_start_line: i64) -> Vec<ExtractedHttpCall> {
    let mut calls = Vec::new();

    // Patterns: new HttpGet("url"), new HttpPost("url"), etc.
    let patterns = [
        (r#"new\s+HttpGet\s*\(\s*"(?P<url>[^"]+)""#, "GET"),
        (r#"new\s+HttpPost\s*\(\s*"(?P<url>[^"]+)""#, "POST"),
        (r#"new\s+HttpPut\s*\(\s*"(?P<url>[^"]+)""#, "PUT"),
        (r#"new\s+HttpDelete\s*\(\s*"(?P<url>[^"]+)""#, "DELETE"),
        (r#"new\s+HttpPatch\s*\(\s*"(?P<url>[^"]+)""#, "PATCH"),
    ];

    for (pattern_str, method) in patterns {
        if let Ok(pattern) = Regex::new(pattern_str) {
            for cap in pattern.captures_iter(source) {
                let url = cap.name("url")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();

                let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
                let line_offset = source[..match_start].matches('\n').count() as i64;

                calls.push(ExtractedHttpCall {
                    http_method: method.to_string(),
                    url,
                    line_number: method_start_line + line_offset,
                });
            }
        }
    }

    calls
}

/// Read source file and extract relevant method body
fn read_method_source(file_path: &str, start_line: i64, _class_name: &str) -> Option<String> {
    let content = fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();

    // Start from method line and read until we find the matching closing brace
    let start_idx = (start_line - 1) as usize;
    if start_idx >= lines.len() {
        return None;
    }

    let mut brace_count = 0;
    let mut method_lines = Vec::new();
    let mut found_open_brace = false;

    for (i, line) in lines.iter().enumerate().skip(start_idx) {
        method_lines.push(*line);

        for ch in line.chars() {
            if ch == '{' {
                brace_count += 1;
                found_open_brace = true;
            } else if ch == '}' {
                brace_count -= 1;
            }
        }

        // If we've found an opening brace and now brace count is 0, we've found the end
        if found_open_brace && brace_count == 0 {
            break;
        }

        // Safety limit - don't read more than 200 lines for a single method
        if i - start_idx > 200 {
            break;
        }
    }

    Some(method_lines.join("\n"))
}

/// Extract path from URL
fn extract_path_from_url(url: &str) -> String {
    // Handle template variables like ${baseUrl}/api/users
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
