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
use std::sync::Arc;
use tracing::{error, info};
use ts_rs::TS;

/// Represents a Java REST API endpoint extracted from the knowledge graph
#[derive(Serialize, Deserialize, TS, Debug, Clone)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaEndpoint {
    pub id: u32,
    pub http_method: String,
    pub path: String,
    pub full_path: String,
    pub consumes: Option<String>,
    pub produces: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub path_params_json: Option<String>,
    pub query_params_json: Option<String>,
    pub request_body_json: Option<String>,
    pub response_body_json: Option<String>,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaEndpointsPathRequest {
    pub workspace_folder_path: String,
    pub project_path: String,
}

#[derive(Deserialize, Serialize, TS, Default, Clone, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaEndpointsQueryRequest {
    pub limit: Option<i32>,
}

#[derive(Serialize, Deserialize, TS, Default, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaEndpointsSuccessResponse {
    pub endpoints: Vec<JavaEndpoint>,
    pub project_info: TSProjectInfo,
}

#[derive(Serialize, Deserialize, TS, Default, Debug)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct JavaEndpointsResponses {
    #[serde(rename = "200")]
    pub ok: Option<JavaEndpointsSuccessResponse>,
    #[serde(rename = "404")]
    pub not_found: Option<StatusResponse>,
    #[serde(rename = "400")]
    pub bad_request: Option<StatusResponse>,
    #[serde(rename = "500")]
    pub internal_server_error: Option<StatusResponse>,
}

pub struct JavaEndpointsEndpointConfig;

impl EndpointConfigTypes for JavaEndpointsEndpointConfig {
    type PathRequest = JavaEndpointsPathRequest;
    type BodyRequest = EmptyRequest;
    type QueryRequest = JavaEndpointsQueryRequest;
    type Response = JavaEndpointsSuccessResponse;
}

define_endpoint! {
    JavaEndpointsEndpoint,
    JavaEndpointsEndpointDef,
    Get,
    "/graph/java-endpoints/{workspace_folder_path}/{project_path}",
    ts_path_type = "\"/api/graph/java-endpoints/{workspace_folder_path}/{project_path}\"",
    config = JavaEndpointsEndpointConfig,
    export_to = "../../../packages/gkg/src/api.ts"
}

impl JavaEndpointsEndpoint {
    pub fn create_success_response(
        endpoints: Vec<JavaEndpoint>,
        project_info: TSProjectInfo,
    ) -> JavaEndpointsSuccessResponse {
        JavaEndpointsSuccessResponse {
            endpoints,
            project_info,
        }
    }

    pub fn create_error_response(status: String) -> StatusResponse {
        StatusResponse { status }
    }
}

pub async fn java_endpoints_handler(
    State(state): State<AppState>,
    Path(path_params): Path<JavaEndpointsPathRequest>,
    Query(query_params): Query<JavaEndpointsQueryRequest>,
) -> impl IntoResponse {
    let input_project_path = decode_url_param!(
        &path_params.project_path,
        "project_path",
        JavaEndpointsEndpoint::create_error_response
    );
    let input_workspace_folder_path = decode_url_param!(
        &path_params.workspace_folder_path,
        "workspace_folder_path",
        JavaEndpointsEndpoint::create_error_response
    );

    let limit = query_params.limit.unwrap_or(1000);

    if input_project_path.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(JavaEndpointsEndpoint::create_error_response(
                "empty_project_path".to_string(),
            )),
        )
            .into_response();
    }

    info!(
        "Received java endpoints request {workspace_folder_path} {project_path} limit={limit}",
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
                Json(JavaEndpointsEndpoint::create_error_response(
                    "project_not_found".to_string(),
                )),
            )
                .into_response();
        }
    };

    // Use the new get_endpoints_query() which directly queries EndpointNode table
    let query = QueryLibrary::get_endpoints_query();

    let mut query_params_map = serde_json::Map::new();
    query_params_map.insert("limit".to_string(), serde_json::Value::Number(limit.into()));

    let query_service = DatabaseQueryingService::new(Arc::clone(&state.database));

    info!(
        "Executing endpoints query for project {} and workspace folder {}, limit={}",
        project_info.project_path, input_workspace_folder_path, limit
    );

    let mut query_result = match query_service.execute_query(
        project_info.database_path.clone(),
        query.query,
        query_params_map,
    ) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to execute endpoints query: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JavaEndpointsEndpoint::create_error_response(format!(
                    "Failed to execute query: {e}"
                ))),
            )
                .into_response();
        }
    };

    let endpoints = match convert_query_result_to_endpoints(&mut query_result) {
        Ok(endpoints) => endpoints,
        Err(e) => {
            error!("Failed to convert query result to endpoints: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(JavaEndpointsEndpoint::create_error_response(format!(
                    "Failed to process results: {e}"
                ))),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(JavaEndpointsEndpoint::create_success_response(
            endpoints,
            to_ts_project_info(&project_info),
        )),
    )
        .into_response()
}

fn convert_query_result_to_endpoints(
    query_result: &mut Box<dyn QueryResult>,
) -> Result<Vec<JavaEndpoint>, Box<dyn std::error::Error>> {
    let mut endpoints = Vec::new();

    while let Some(row) = query_result.next() {
        // Extract all fields from the EndpointNode query result
        let id = row.get_int_value(0)? as u32;
        let http_method = row.get_string_value(1)?;
        let path = row.get_string_value(2)?;
        let full_path = row.get_string_value(3)?;

        // Convert empty strings to None for optional fields
        let consumes = row.get_string_value(4).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let produces = row.get_string_value(5).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let description = row.get_string_value(6).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let deprecated = row.get_bool_value(7)?;
        let path_params_json = row.get_string_value(8).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let query_params_json = row.get_string_value(9).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let request_body_json = row.get_string_value(10).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });
        let response_body_json = row.get_string_value(11).ok().and_then(|s| if s.is_empty() { None } else { Some(s) });

        let file_path = row.get_string_value(12)?;
        let start_line = row.get_int_value(13)? as i32;
        let end_line = row.get_int_value(14)? as i32;

        endpoints.push(JavaEndpoint {
            id,
            http_method,
            path,
            full_path,
            consumes,
            produces,
            description,
            deprecated,
            path_params_json,
            query_params_json,
            request_body_json,
            response_body_json,
            file_path,
            start_line,
            end_line,
        });
    }

    Ok(endpoints)
}
