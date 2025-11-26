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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{error, info};
use ts_rs::TS;

/// Represents a method node in the call tree
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct MethodNode {
    pub fqn: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
}

/// Type of method call
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
#[serde(rename_all = "PascalCase")]
pub enum CallType {
    Direct,
    Ambiguous,
    Interface,
    Abstract,
}

/// Represents a node in the call tree
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct CallTreeNode {
    pub method_fqn: String,
    pub method_name: String,
    pub class_name: String,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub depth: usize,
    pub call_type: CallType,
    pub is_external: bool,
    pub children: Vec<CallTreeNode>,
}

/// Flow statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct FlowStatistics {
    pub total_methods: usize,
    pub max_depth: usize,
    pub direct_calls: usize,
    pub ambiguous_calls: usize,
    pub external_calls: usize,
    pub has_recursive_calls: bool,
}

/// Endpoint flow response
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct EndpointFlowResponse {
    pub endpoint_id: u32,
    pub endpoint_http_method: String,
    pub endpoint_path: String,
    pub endpoint_full_path: String,
    pub root_method: MethodNode,
    pub call_tree: Vec<CallTreeNode>,
    pub statistics: FlowStatistics,
    pub project_info: TSProjectInfo,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct EndpointFlowPathRequest {
    pub workspace_folder_path: String,
    pub project_path: String,
    pub endpoint_id: String,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct EndpointFlowQueryRequest {
    pub max_depth: Option<i32>,
}

#[derive(Serialize, Deserialize, TS, Default, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct EndpointFlowResponses {
    #[serde(rename = "200")]
    pub ok: Option<EndpointFlowResponse>,
    #[serde(rename = "404")]
    pub not_found: Option<StatusResponse>,
    #[serde(rename = "400")]
    pub bad_request: Option<StatusResponse>,
    #[serde(rename = "500")]
    pub internal_server_error: Option<StatusResponse>,
}

pub struct EndpointFlowEndpointConfig;

impl EndpointConfigTypes for EndpointFlowEndpointConfig {
    type PathRequest = EndpointFlowPathRequest;
    type BodyRequest = EmptyRequest;
    type QueryRequest = EndpointFlowQueryRequest;
    type Response = EndpointFlowResponse;
}

define_endpoint! {
    EndpointFlowEndpoint,
    EndpointFlowEndpointDef,
    Get,
    "/graph/endpoint-flow/{workspace_folder_path}/{project_path}/{endpoint_id}",
    ts_path_type = "\"/api/graph/endpoint-flow/{workspace_folder_path}/{project_path}/{endpoint_id}\"",
    config = EndpointFlowEndpointConfig,
    export_to = "../../../packages/gkg/src/api.ts"
}

impl EndpointFlowEndpoint {
    pub fn create_error_response(status: String) -> StatusResponse {
        StatusResponse { status }
    }
}

pub async fn endpoint_flow_handler(
    State(state): State<AppState>,
    Path(path_params): Path<EndpointFlowPathRequest>,
    Query(query_params): Query<EndpointFlowQueryRequest>,
) -> impl IntoResponse {
    let input_project_path = decode_url_param!(
        &path_params.project_path,
        "project_path",
        EndpointFlowEndpoint::create_error_response
    );
    let input_workspace_folder_path = decode_url_param!(
        &path_params.workspace_folder_path,
        "workspace_folder_path",
        EndpointFlowEndpoint::create_error_response
    );
    let endpoint_id_str = decode_url_param!(
        &path_params.endpoint_id,
        "endpoint_id",
        EndpointFlowEndpoint::create_error_response
    );

    let endpoint_id: u32 = match endpoint_id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(EndpointFlowEndpoint::create_error_response(
                    "invalid_endpoint_id".to_string(),
                )),
            )
                .into_response();
        }
    };

    let max_depth = query_params.max_depth.unwrap_or(5).clamp(1, 10);

    info!(
        "Received endpoint flow request for endpoint_id={} max_depth={}",
        endpoint_id, max_depth
    );

    let project_info = match state
        .workspace_manager
        .get_project_info(&input_workspace_folder_path, &input_project_path)
    {
        Some(info) => info,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(EndpointFlowEndpoint::create_error_response(
                    "project_not_found".to_string(),
                )),
            )
                .into_response();
        }
    };

    let query_service = DatabaseQueryingService::new(Arc::clone(&state.database));

    // First, get the root method from endpoint
    let root_query = r#"
        MATCH (method:DefinitionNode)-[:ENDPOINT_RELATIONSHIPS]->(ep:EndpointNode)
        WHERE ep.id = $endpoint_id AND method.definition_type = 'Method'
        RETURN method.fqn as root_method_fqn,
               method.name as root_method_name,
               method.primary_file_path as root_method_file_path,
               method.start_line as root_method_start_line,
               method.end_line as root_method_end_line
        LIMIT 1
    "#;

    let mut root_params = serde_json::Map::new();
    root_params.insert("endpoint_id".to_string(), serde_json::Value::Number(endpoint_id.into()));

    let mut root_result = match query_service.execute_query(
        project_info.database_path.clone(),
        root_query.to_string(),
        root_params,
    ) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to get root method: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(EndpointFlowEndpoint::create_error_response(format!(
                    "Failed to get root method: {e}"
                ))),
            )
                .into_response();
        }
    };

    let root_method = if let Some(row) = root_result.next() {
        match (|| -> Result<MethodNode, Box<dyn std::error::Error>> {
            let fqn = row.get_string_value(0)?;
            let name = row.get_string_value(1)?;
            let file_path = row.get_string_value(2)?;
            let start_line = row.get_int_value(3)? as i32;
            let end_line = row.get_int_value(4)? as i32;
            Ok(MethodNode {
                fqn,
                name,
                file_path,
                start_line,
                end_line,
            })
        })() {
            Ok(method) => method,
            Err(e) => {
                error!("Failed to parse root method: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(EndpointFlowEndpoint::create_error_response(format!(
                        "Failed to parse root method: {e}"
                    ))),
                )
                    .into_response();
            }
        }
    } else {
        return (
            StatusCode::NOT_FOUND,
            Json(EndpointFlowEndpoint::create_error_response(
                "No root method found for endpoint".to_string(),
            )),
        )
            .into_response();
    };

    let query = QueryLibrary::get_endpoint_flow_query();
    let mut query_params_map = serde_json::Map::new();
    query_params_map.insert("endpoint_id".to_string(), serde_json::Value::Number(endpoint_id.into()));
    query_params_map.insert("max_depth".to_string(), serde_json::Value::Number(max_depth.into()));
    query_params_map.insert("limit".to_string(), serde_json::Value::Number(1000.into()));

    let mut query_result = match query_service.execute_query(
        project_info.database_path.clone(),
        query.query,
        query_params_map,
    ) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to execute endpoint flow query: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(EndpointFlowEndpoint::create_error_response(format!(
                    "Failed to execute query: {e}"
                ))),
            )
                .into_response();
        }
    };

    let flow_response = match build_flow_response(&mut query_result, endpoint_id, &project_info, root_method) {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to build flow response: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(EndpointFlowEndpoint::create_error_response(format!(
                    "Failed to process results: {e}"
                ))),
            )
                .into_response();
        }
    };

    (StatusCode::OK, Json(flow_response)).into_response()
}

// Structure to hold method call information
#[derive(Debug, Clone)]
struct MethodCallInfo {
    fqn: String,
    name: String,
    class_name: String,
    file_path: String,
    start_line: i32,
    end_line: i32,
    depth: usize,
    relationship_type: String,
}

fn build_flow_response(
    query_result: &mut Box<dyn QueryResult>,
    endpoint_id: u32,
    project_info: &workspace_manager::ProjectInfo,
    root_method: MethodNode,
) -> Result<EndpointFlowResponse, Box<dyn std::error::Error>> {
    let mut endpoint_info: Option<(String, String, String, String)> = None;
    let mut call_map: HashMap<String, Vec<MethodCallInfo>> = HashMap::new();
    let mut all_methods: HashSet<String> = HashSet::new();
    let mut direct_calls = 0;
    let mut ambiguous_calls = 0;
    let mut external_calls = 0;
    let mut max_depth = 0;

    // Get project path to determine if a call is external
    let project_path = &project_info.project_path;

    // Add root method to all_methods
    all_methods.insert(root_method.fqn.clone());

    while let Some(row) = query_result.next() {
        // Extract endpoint info (same for all rows)
        if endpoint_info.is_none() {
            let http_method = row.get_string_value(1)?;
            let path = row.get_string_value(2)?;
            let full_path = row.get_string_value(3)?;
            let file_path = row.get_string_value(4)?;
            endpoint_info = Some((http_method, path, full_path, file_path));
        }

        // Extract called method
        if let Ok(caller_fqn) = row.get_string_value(7) {
            if let Ok(called_fqn) = row.get_string_value(8) {
                if !called_fqn.is_empty() {
                    let called_name = row.get_string_value(9)?;
                    let called_class_name = row.get_string_value(10)?;
                    let called_file_path = row.get_string_value(11)?;
                    let called_start_line = row.get_int_value(12)? as i32;
                    let called_end_line = row.get_int_value(13)? as i32;
                    let depth = row.get_int_value(14)? as usize;
                    let relationship_type = row.get_string_value(15).unwrap_or_default();

                    all_methods.insert(called_fqn.clone());

                    // Determine if external (file not in project path)
                    let is_external = !called_file_path.starts_with(project_path);
                    if is_external {
                        external_calls += 1;
                    }

                    // Count call types
                    if relationship_type == "AMBIGUOUSLY_CALLS" {
                        ambiguous_calls += 1;
                    } else {
                        direct_calls += 1;
                    }

                    if depth > max_depth {
                        max_depth = depth;
                    }

                    // Use caller_fqn as the key to build proper parent-child relationships
                    call_map
                        .entry(caller_fqn.clone())
                        .or_insert_with(Vec::new)
                        .push(MethodCallInfo {
                            fqn: called_fqn,
                            name: called_name,
                            class_name: called_class_name,
                            file_path: called_file_path,
                            start_line: called_start_line,
                            end_line: called_end_line,
                            depth,
                            relationship_type,
                        });
                }
            }
        }
    }

    let (endpoint_http_method, endpoint_path, endpoint_full_path, _endpoint_file_path) =
        endpoint_info.ok_or("No endpoint found")?;

    // Build call tree
    let call_tree = build_call_tree(
        &root_method.fqn,
        &call_map,
        &mut HashSet::new(),
        0,
        project_path,
    );

    // Detect recursive calls
    let has_recursive_calls = detect_cycles(&call_map);

    let statistics = FlowStatistics {
        total_methods: all_methods.len(),
        max_depth,
        direct_calls,
        ambiguous_calls,
        external_calls,
        has_recursive_calls,
    };

    Ok(EndpointFlowResponse {
        endpoint_id,
        endpoint_http_method,
        endpoint_path,
        endpoint_full_path,
        root_method,
        call_tree,
        statistics,
        project_info: to_ts_project_info(project_info),
    })
}

fn build_call_tree(
    current_fqn: &str,
    call_map: &HashMap<String, Vec<MethodCallInfo>>,
    visited: &mut HashSet<String>,
    current_depth: usize,
    project_path: &str,
) -> Vec<CallTreeNode> {
    let mut tree = Vec::new();

    if let Some(calls) = call_map.get(current_fqn) {
        for call_info in calls {
            if call_info.depth != current_depth + 1 {
                continue;
            }

            // Detect recursion
            if visited.contains(&call_info.fqn) {
                continue;
            }

            visited.insert(call_info.fqn.clone());

            // Determine call type from relationship_type
            let call_type = match call_info.relationship_type.as_str() {
                "CALLS" => CallType::Direct,
                "AMBIGUOUSLY_CALLS" => CallType::Ambiguous,
                // Can add Interface/Abstract detection based on additional metadata if available
                _ => CallType::Direct,
            };

            // Determine if external
            let is_external = !call_info.file_path.starts_with(project_path);

            let children = build_call_tree(
                &call_info.fqn,
                call_map,
                visited,
                current_depth + 1,
                project_path,
            );

            tree.push(CallTreeNode {
                method_fqn: call_info.fqn.clone(),
                method_name: call_info.name.clone(),
                class_name: call_info.class_name.clone(),
                file_path: call_info.file_path.clone(),
                start_line: call_info.start_line,
                end_line: call_info.end_line,
                depth: call_info.depth,
                call_type,
                is_external,
                children,
            });

            visited.remove(&call_info.fqn);
        }
    }

    tree
}

fn detect_cycles(call_map: &HashMap<String, Vec<MethodCallInfo>>) -> bool {
    fn has_cycle(
        node: &str,
        call_map: &HashMap<String, Vec<MethodCallInfo>>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(calls) = call_map.get(node) {
            for call_info in calls {
                if !visited.contains(&call_info.fqn) {
                    if has_cycle(&call_info.fqn, call_map, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(&call_info.fqn) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        false
    }

    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();

    for node in call_map.keys() {
        if !visited.contains(node) {
            if has_cycle(node, call_map, &mut visited, &mut rec_stack) {
                return true;
            }
        }
    }

    false
}
