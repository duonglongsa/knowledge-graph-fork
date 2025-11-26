use anyhow::Result;
use database::kuzu::database::KuzuDatabase;
use event_bus::EventBus;
use http_server_desktop::{PREFERRED_PORT, run};
use monitoring::{LogMode, init_logging};
use std::env;
use std::sync::Arc;
use tracing::info;
use workspace_manager::WorkspaceManager;

#[tokio::main]
async fn main() -> Result<()> {
    init_logging(LogMode::Cli, true).unwrap();

    let port = env::var("DEV_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(PREFERRED_PORT);
    let enable_reindexing = std::env::args().any(|arg| arg == "--enable-reindexing");
    info!("🚀 Development server starting on port {port} with reindexing: {enable_reindexing}");

    let workspace_manager = Arc::new(WorkspaceManager::new_system_default().unwrap());
    let event_bus = Arc::new(EventBus::new());
    let database = Arc::new(KuzuDatabase::new());

    let mcp_configuration =
        if let Some(idx) = std::env::args().position(|arg| arg == "--mcp-configuration") {
            // Try to get the next argument as the path
            if let Some(path) = std::env::args().nth(idx + 1) {
                let path = std::path::PathBuf::from(path);
                Arc::new(mcp::configuration::read_mcp_configuration(path))
            } else {
                // If no path is provided after the flag, use default
                Arc::new(mcp::configuration::get_or_create_mcp_configuration(
                    workspace_manager.clone(),
                ))
            }
        } else {
            Arc::new(mcp::configuration::get_or_create_mcp_configuration(
                workspace_manager.clone(),
            ))
        };

    run(
        port,
        enable_reindexing,
        database,
        workspace_manager,
        event_bus,
        mcp_configuration,
        None,
    )
    .await
}
