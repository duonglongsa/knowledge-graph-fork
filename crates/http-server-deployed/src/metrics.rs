use axum::{extract::Request, middleware::Next, response::Response};
use monitoring::metrics::{HTTP_REQUEST_DURATION_SECONDS, HTTP_RESPONSES_TOTAL};
use std::time::Instant;

pub async fn request_metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    let start = Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&[&method, &path])
        .observe(duration);

    HTTP_RESPONSES_TOTAL
        .with_label_values(&[&method, &path, &status])
        .inc();

    response
}
