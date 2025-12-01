use crate::AppState;
use crate::contract::{EmptyRequest, EndpointConfigTypes};
use crate::define_endpoint;
use crate::endpoints::shared::StatusResponse;
use crate::queue::job::JobPriority;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use ts_rs::TS;

#[derive(Deserialize, Serialize, TS, Default, Clone)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct ContextExclusionBodyRequest {
    #[serde(default)]
    pub project_path: String,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct ContextExclusionSuccessResponse {
    #[serde(default)]
    pub project_path: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub reindexing_triggered: bool,
}

#[derive(Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../packages/gkg/src/api.ts")]
pub struct ContextExclusionResponses {
    #[serde(rename = "200")]
    pub ok: ContextExclusionSuccessResponse,
    #[serde(rename = "400")]
    pub bad_request: StatusResponse,
    #[serde(rename = "404")]
    pub not_found: StatusResponse,
    #[serde(rename = "500")]
    pub internal_server_error: StatusResponse,
}

pub struct ContextExclusionEndpointConfig;

impl EndpointConfigTypes for ContextExclusionEndpointConfig {
    type PathRequest = EmptyRequest;
    type BodyRequest = ContextExclusionBodyRequest;
    type QueryRequest = EmptyRequest;
    type Response = ContextExclusionResponses;
}

define_endpoint! {
    ContextExclusionEndpoint,
    ContextExclusionEndpointDef,
    Post,
    "/project/context-exclusion",
    ts_path_type = "\"/api/project/context-exclusion\"",
    config = ContextExclusionEndpointConfig,
    export_to = "../../../packages/gkg/src/api.ts"
}

pub async fn context_exclusion_handler(
    State(state): State<AppState>,
    Json(payload): Json<ContextExclusionBodyRequest>,
) -> impl IntoResponse {
    info!(
        "Configuring context exclusion patterns for project: {}",
        payload.project_path
    );

    // Look up project to get workspace folder path
    let project_info = match state
        .workspace_manager
        .get_project_for_path(&payload.project_path)
    {
        Some(info) => info,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(StatusResponse {
                    status: "project_not_found".to_string(),
                }),
            )
                .into_response();
        }
    };

    let workspace_folder_path = &project_info.workspace_folder_path;

    // Get current patterns to check if they changed
    let patterns_changed = {
        let current_patterns = state
            .workspace_manager
            .get_project_context_exclusion_patterns(workspace_folder_path, &payload.project_path);
        current_patterns != payload.patterns
    };

    // Update project metadata with context exclusion patterns
    if let Err(e) = state
        .workspace_manager
        .update_project_context_exclusion_patterns(
            workspace_folder_path,
            &payload.project_path,
            payload.patterns.clone(),
        )
    {
        error!("Failed to update project context exclusion patterns: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(StatusResponse {
                status: format!("Failed to update configuration: {e}"),
            }),
        )
            .into_response();
    }

    // If patterns changed, trigger re-indexing to clean up excluded files
    if patterns_changed {
        info!(
            "Context exclusion patterns changed, triggering clean re-index for project: {}",
            payload.project_path
        );

        // Dispatch a high-priority indexing job for the workspace
        let job = crate::queue::job::Job::IndexWorkspaceFolder {
            workspace_folder_path: workspace_folder_path.to_string(),
            priority: JobPriority::High,
            java_property_files: Vec::new(),
        };

        if let Err(e) = state.job_dispatcher.dispatch(job).await {
            error!(
                "Failed to dispatch re-indexing job for project {}: {}",
                payload.project_path, e
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StatusResponse {
                    status: format!("Failed to schedule re-indexing: {e}"),
                }),
            )
                .into_response();
        }
    }

    info!(
        "Successfully configured {} context exclusion patterns for project: {}",
        payload.patterns.len(),
        payload.project_path
    );

    (
        StatusCode::OK,
        Json(ContextExclusionSuccessResponse {
            project_path: payload.project_path,
            patterns: payload.patterns,
            reindexing_triggered: patterns_changed,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::post};
    use axum_test::TestServer;
    use database::kuzu::database::KuzuDatabase;
    use event_bus::EventBus;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;
    use workspace_manager::WorkspaceManager;

    fn create_test_git_repo(path: &std::path::Path) {
        fs::create_dir_all(path).unwrap();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();

        fs::write(path.join("README.md"), "# Test Repo").unwrap();
        fs::write(path.join("main.rb"), "puts 'Hello, World!'").unwrap();
        fs::write(path.join("test.rb"), "# test file").unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    async fn create_test_app() -> (TestServer, TempDir, TempDir, Arc<WorkspaceManager>) {
        let temp_data_dir = TempDir::new().unwrap();
        let workspace_manager = Arc::new(
            WorkspaceManager::new_with_directory(temp_data_dir.path().to_path_buf()).unwrap(),
        );
        let event_bus = Arc::new(EventBus::new());
        let database = Arc::new(KuzuDatabase::new());

        let job_dispatcher = Arc::new(crate::queue::dispatch::JobDispatcher::new(
            workspace_manager.clone(),
            event_bus.clone(),
            Arc::clone(&database),
        ));

        let state = crate::AppState {
            database: Arc::clone(&database),
            workspace_manager: workspace_manager.clone(),
            event_bus,
            job_dispatcher,
        };

        let app = Router::new()
            .route(
                "/project/context-exclusion",
                post(context_exclusion_handler),
            )
            .with_state(state);

        let temp_workspace = TempDir::new().unwrap();
        (
            TestServer::new(app).unwrap(),
            temp_data_dir,
            temp_workspace,
            workspace_manager,
        )
    }

    #[tokio::test]
    async fn test_context_exclusion_project_not_found() {
        let (server, _temp_data_dir, _temp_workspace, _workspace_manager) = create_test_app().await;

        let request_body = ContextExclusionBodyRequest {
            project_path: "/nonexistent/project".to_string(),
            patterns: vec!["*.test.rb".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status(StatusCode::NOT_FOUND);
        let body: StatusResponse = response.json();
        assert_eq!(body.status, "project_not_found");
    }

    #[tokio::test]
    async fn test_context_exclusion_success() {
        let (server, _temp_data_dir, temp_workspace, workspace_manager) = create_test_app().await;

        // Create a test project
        let project_path = temp_workspace.path().join("test_project");
        create_test_git_repo(&project_path);

        // Register the workspace using the same workspace_manager from the server
        workspace_manager
            .register_workspace_folder(temp_workspace.path())
            .unwrap();

        let project_path_canonical = project_path.canonicalize().unwrap();

        let request_body = ContextExclusionBodyRequest {
            project_path: project_path_canonical.to_string_lossy().to_string(),
            patterns: vec!["*.test.rb".to_string(), "fixtures/**".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status_ok();
        let body: ContextExclusionSuccessResponse = response.json();
        assert_eq!(body.patterns.len(), 2);
        assert!(body.patterns.contains(&"*.test.rb".to_string()));
        assert!(body.patterns.contains(&"fixtures/**".to_string()));
        assert!(body.reindexing_triggered); // Should trigger reindex on first set
    }

    #[tokio::test]
    async fn test_context_exclusion_patterns_changed_triggers_cleanup() {
        let (server, _temp_data_dir, temp_workspace, workspace_manager) = create_test_app().await;

        // Create a test project
        let project_path = temp_workspace.path().join("test_project");
        create_test_git_repo(&project_path);

        // Register the workspace
        workspace_manager
            .register_workspace_folder(temp_workspace.path())
            .unwrap();

        let project_path_canonical = project_path.canonicalize().unwrap();

        // Set initial patterns
        let request_body = ContextExclusionBodyRequest {
            project_path: project_path_canonical.to_string_lossy().to_string(),
            patterns: vec!["*.test.rb".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status_ok();
        let body: ContextExclusionSuccessResponse = response.json();
        assert!(body.reindexing_triggered);

        // Update patterns - should trigger cleanup and re-index
        let request_body = ContextExclusionBodyRequest {
            project_path: project_path_canonical.to_string_lossy().to_string(),
            patterns: vec!["**/*test*".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status_ok();
        let body: ContextExclusionSuccessResponse = response.json();
        assert_eq!(body.patterns.len(), 1);
        assert!(body.patterns.contains(&"**/*test*".to_string()));
        assert!(body.reindexing_triggered); // Should trigger reindex when patterns change
    }

    #[tokio::test]
    async fn test_context_exclusion_same_patterns_no_reindex() {
        let (server, _temp_data_dir, temp_workspace, workspace_manager) = create_test_app().await;

        // Create a test project
        let project_path = temp_workspace.path().join("test_project");
        create_test_git_repo(&project_path);

        // Register the workspace
        workspace_manager
            .register_workspace_folder(temp_workspace.path())
            .unwrap();

        let project_path_canonical = project_path.canonicalize().unwrap();

        // Set initial patterns
        let request_body = ContextExclusionBodyRequest {
            project_path: project_path_canonical.to_string_lossy().to_string(),
            patterns: vec!["*.test.rb".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status_ok();
        let body: ContextExclusionSuccessResponse = response.json();
        assert!(body.reindexing_triggered);

        // Set same patterns again - should NOT trigger re-index
        let request_body = ContextExclusionBodyRequest {
            project_path: project_path_canonical.to_string_lossy().to_string(),
            patterns: vec!["*.test.rb".to_string()],
        };

        let response = server
            .post("/project/context-exclusion")
            .json(&request_body)
            .await;

        response.assert_status_ok();
        let body: ContextExclusionSuccessResponse = response.json();
        assert!(!body.reindexing_triggered); // Should NOT trigger reindex when patterns unchanged
    }
}
