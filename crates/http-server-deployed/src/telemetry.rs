use axum::{extract::Request, middleware::Next, response::Response};
use opentelemetry::global;
use opentelemetry::trace::Tracer;

pub use monitoring::telemetry::init_otel;

/// Middleware to create OpenTelemetry spans for HTTP requests
pub async fn tracing_middleware(req: Request, next: Next) -> Response {
    let tracer = global::tracer("http-server-deployed");

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let span_name = format!("{method} {path}");

    tracer
        .in_span(span_name, |_cx| async move { next.run(req).await })
        .await
}
