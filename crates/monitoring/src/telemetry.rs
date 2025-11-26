use opentelemetry::global;
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::env;
use std::error::Error;
use tracing::info;

/// Initialize OpenTelemetry tracing with OTLP exporter
pub fn init_otel(mode: &str) -> Result<(), Box<dyn Error>> {
    // Default to local collector via gRPC unless overridden by env
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let protocol_str =
        env::var("OTEL_EXPORTER_OTLP_PROTOCOL").unwrap_or_else(|_| "grpc".to_string());

    let protocol = match protocol_str.as_str() {
        "http" => Protocol::HttpBinary,
        "grpc" => Protocol::Grpc,
        _ => Protocol::HttpBinary,
    };

    // Create OTLP exporter. For HTTP, OTEL_EXPORTER_OTLP_ENDPOINT must be a full OTLP HTTP URL
    // ending with v1/traces (e.g., https://gitlab.com/api/v4/projects/69095239/observability/v1/traces),
    // and any auth headers must be supplied via OTEL_EXPORTER_OTLP_HEADERS.
    // For gRPC, we default to http://localhost:4317.
    let service_name = format!("gkg-server-deployed-{mode}");
    let resource = Resource::builder().with_service_name(service_name).build();

    match protocol {
        Protocol::Grpc => {
            let tracer = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.clone())
                .with_protocol(Protocol::Grpc)
                .build()?;
            let provider = SdkTracerProvider::builder()
                .with_batch_exporter(tracer)
                .with_resource(resource)
                .build();
            global::set_tracer_provider(provider);
        }
        _ => {
            let tracer = opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.clone())
                .build()?;
            let provider = SdkTracerProvider::builder()
                .with_batch_exporter(tracer)
                .with_resource(resource)
                .build();
            global::set_tracer_provider(provider);
        }
    }

    info!(
        "OpenTelemetry tracing initialized with OTLP endpoint: {} (protocol: {})",
        endpoint, protocol_str
    );
    Ok(())
}
