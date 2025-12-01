pub mod api;
pub mod contract;
pub mod endpoints;
pub mod queue;
pub mod watcher;

#[cfg(test)]
pub mod testing;

use crate::{
    contract::EndpointContract,
    endpoints::{
        context_exclusion::{ContextExclusionEndpoint, context_exclusion_handler},
        events::{EventsEndpoint, events_handler},
        graph::{
            endpoint_flow::{EndpointFlowEndpoint, endpoint_flow_handler},
            graph_endpoints::{JavaEndpointsEndpoint, java_endpoints_handler},
            graph_initial::{GraphInitialEndpoint, graph_initial_handler},
            graph_neighbors::{GraphNeighborsEndpoint, graph_neighbors_handler},
            graph_search::{GraphSearchEndpoint, graph_search_handler},
            graph_stats::{GraphStatsEndpoint, graph_stats_handler},
            service_calls::{JavaServiceCallsEndpoint, java_service_calls_handler},
        },
        health::health_handler,
        info::{InfoEndpoint, info_handler},
        workspace_delete::{WorkspaceDeleteEndpoint, delete_handler},
        workspace_index::{WorkspaceIndexEndpoint, index_handler},
        workspace_list::{WorkspaceListEndpoint, workspace_list_handler},
    },
    queue::dispatch::JobDispatcher,
    watcher::Watcher,
};

use anyhow::Result;
use axum::{
    Router,
    routing::{delete, get, post},
};
use axum_embed::ServeEmbed;
use database::querying::service::DatabaseQueryingService;
use database::{kuzu::database::KuzuDatabase, querying::QueryingService};
use event_bus::EventBus;
use mcp::{configuration::McpConfiguration, http::mcp_http_service, sse::mcp_sse_router};
use rust_embed::Embed;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tracing::info;
use workspace_manager::WorkspaceManager;

#[derive(Clone)]
pub struct AppState {
    pub database: Arc<KuzuDatabase>,
    pub workspace_manager: Arc<WorkspaceManager>,
    pub event_bus: Arc<EventBus>,
    pub job_dispatcher: Arc<JobDispatcher>,
}

#[cfg(feature = "no-frontend")]
#[derive(Embed, Clone)]
#[folder = "../../packages/frontend/dist"]
#[allow_missing = true]
struct Assets;

#[cfg(not(feature = "no-frontend"))]
#[derive(Embed, Clone)]
#[folder = "../../packages/frontend/dist"]
#[allow_missing = false]
struct Assets;

pub async fn run(
    port: u16,
    enable_reindexing: bool,
    database: Arc<KuzuDatabase>,
    workspace_manager: Arc<WorkspaceManager>,
    event_bus: Arc<EventBus>,
    mcp_configuration: Arc<McpConfiguration>,
    on_ready: Option<Box<dyn FnOnce(u16) + Send>>,
) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let cors_layer = CorsLayer::permissive();

    let job_dispatcher = Arc::new(JobDispatcher::new(
        workspace_manager.clone(),
        event_bus.clone(),
        Arc::clone(&database),
    ));

    let query_service: Arc<dyn QueryingService> =
        Arc::new(DatabaseQueryingService::new(Arc::clone(&database)));

    let watcher = Arc::new(Watcher::new(
        workspace_manager.clone(),
        job_dispatcher.clone(),
        None,
    ));
    if enable_reindexing {
        watcher.start().await;
    }

    let state = AppState {
        database: Arc::clone(&database),
        workspace_manager: workspace_manager.clone(),
        event_bus: Arc::clone(&event_bus),
        job_dispatcher,
    };

    let serve_assets = ServeEmbed::<Assets>::new();

    let mcp_http_router = mcp_http_service(
        Arc::clone(&query_service),
        Arc::clone(&workspace_manager),
        Arc::clone(&database),
        Arc::clone(&event_bus),
        Arc::clone(&mcp_configuration),
    );
    let (mcp_sse_router, mcp_sse_cancellation_token) = mcp_sse_router(
        addr,
        Arc::clone(&query_service),
        Arc::clone(&workspace_manager),
        Arc::clone(&database),
        Arc::clone(&event_bus),
        Arc::clone(&mcp_configuration),
    );

    let api_router = Router::new()
        .route(
            InfoEndpoint::PATH,
            get({
                let shared_port = port;
                move || info_handler(shared_port)
            }),
        )
        .route(WorkspaceIndexEndpoint::PATH, post(index_handler))
        .route(WorkspaceDeleteEndpoint::PATH, delete(delete_handler))
        .route(EventsEndpoint::PATH, get(events_handler))
        .route(WorkspaceListEndpoint::PATH, get(workspace_list_handler))
        .route(GraphInitialEndpoint::PATH, get(graph_initial_handler))
        .route(GraphNeighborsEndpoint::PATH, get(graph_neighbors_handler))
        .route(GraphSearchEndpoint::PATH, get(graph_search_handler))
        .route(GraphStatsEndpoint::PATH, get(graph_stats_handler))
        .route(
            ContextExclusionEndpoint::PATH,
            post(context_exclusion_handler),
        )
        .route(JavaEndpointsEndpoint::PATH, get(java_endpoints_handler))
        .route(JavaServiceCallsEndpoint::PATH, get(java_service_calls_handler))
        .route(EndpointFlowEndpoint::PATH, get(endpoint_flow_handler))
        .with_state(state);

    let app = Router::new()
        .route("/health", get(health_handler))
        .nest("/api", api_router)
        .nest_service("/mcp", mcp_http_router)
        .nest_service("/mcp/sse", mcp_sse_router)
        .fallback_service(serve_assets)
        .layer(ServiceBuilder::new().layer(cors_layer));

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "Port {} is already in use. The Knowledge Graph server may already be running.\n\
                 Check if a server is running with: gkg server stop\n\
                 Or specify a different port with: --port <port_number>",
                port
            )
        } else {
            anyhow::anyhow!("Failed to bind to port {}: {}", port, e)
        }
    })?;

    let port = listener.local_addr()?.port();
    info!("HTTP server listening on http://localhost:{}", port);

    if let Some(callback) = on_ready {
        callback(port);
    }

    // Set up graceful shutdown
    let server = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());

    // Run the server and handle shutdown
    let result = server.await;

    // Cancel MCP SSE server
    mcp_sse_cancellation_token.cancel();

    // Log shutdown completion
    info!("HTTP server shut down gracefully");

    result.map_err(Into::into)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown...");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown...");
        },
    }
}

// The preferred port is an easter egg from "knowledge graph":
// 'k' -> 0x6b, 'g' -> 0x67 => 0x6b67 => 27495
pub const PREFERRED_PORT: u16 = 27495;
