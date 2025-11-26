# GitLab Knowldege Graph Observability Stack

This directory contains the local observability stack for the GKG (GitLab Knowledge Graph) project. It mirrors the production setup with Prometheus for metrics collection, Grafana for visualization, and Grafana Mimir for long-term metrics storage.

## Quick Start

### Start the observability stack
```bash
mise run observability:up
```

### Stop the observability stack
```bash
mise run observability:down
```

## Components

### Prometheus
- **URL**: http://localhost:9090
- **Purpose**: Metrics collection and short-term storage
- Scrapes metrics from the GKG HTTP server every 15 seconds
- Forwards metrics to Mimir for long-term storage via remote write

### Grafana
- **URL**: http://localhost:3001
- **Credentials**: admin / admin
- **Purpose**: Visualization and dashboards
- Pre-configured with:
  - Prometheus datasource (default)
  - Mimir datasource for long-term metrics
  - GKG Overview Dashboard

### Grafana Mimir
- **URL**: http://localhost:9009
- **Purpose**: Long-term metrics storage and querying
- Configured in single-process mode for local development
- Stores metrics in the local filesystem

### Jaeger (Traces)
- **URL**: http://localhost:16686
- **Purpose**: Distributed tracing UI for spans sent via OTLP (gRPC or HTTP)
- Receivers enabled on: `4317` (OTLP gRPC) and `4318` (OTLP HTTP)
- No persistence (in-memory) for local development

## Configuration

### Prometheus Configuration
Edit `prometheus/prometheus.yml` to:
- Add new scrape targets
- Adjust scrape intervals
- Configure alerting rules

**Note**: By default, Prometheus expects the GKG HTTP server to be running on `host.docker.internal:8080` (the default port). If you use the `--bind` flag with a different port, update the target in `prometheus.yml` to match.

### Grafana Dashboards
- Dashboards are located in `grafana/provisioning/dashboards/json/`
- New dashboards can be added to this directory and will be auto-loaded
- Edit existing dashboards through the Grafana UI and save

### Mimir Configuration
Edit `mimir/mimir.yaml` to adjust:
- Storage backend settings
- Ingestion rate limits
- Retention policies

## Data Persistence

All metrics data is stored in Docker volumes:
- `prometheus-data` - Prometheus TSDB
- `mimir-data` - Mimir blocks and metadata
- `grafana-data` - Grafana dashboards and settings

To reset all data:
```bash
mise run observability:clean
```

## Troubleshooting

### Metrics not showing up in Grafana
1. Check if the GKG HTTP server is running and accessible
2. Verify Prometheus can scrape the metrics endpoint:
   - Go to http://localhost:9090/targets
   - Check if `gkg-http-server-deployed` target is UP
3. Check container logs:
   ```bash
   docker-compose logs -f prometheus
   docker-compose logs -f grafana
   docker-compose logs -f mimir
   ```

### Can't access services
- Ensure no other services are using ports 3001, 9009, or 9090
- Check if Docker is running
- Verify containers are running: `docker-compose ps`

### Connection refused from Prometheus to GKG server
- On macOS/Windows, Docker uses `host.docker.internal` to access host services
- On Linux, you may need to use the host's IP address or configure the docker-compose to use host network mode

## Running the HTTP Server

The deployed HTTP server now defaults to TCP binding on `127.0.0.1:8080`:

```bash
# Run with default port (8080)
cargo run --bin http-server-deployed -- --secret-path /path/to/secret

# Run on a different port
cargo run --bin http-server-deployed -- --bind 127.0.0.1:9090 --secret-path /path/to/secret
```


## OpenTelemetry Traces and Jaeger UI

The deployed server exports traces over OTLP to Jaeger. We use the
`opentelemetry-otlp` crate; see the docs for configuration details: [opentelemetry-otlp docs](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/).

### Defaults
- **Protocol**: gRPC
- **Endpoint**: `http://localhost:4317`
- **Service name**: `gkg-server-deployed-{mode}` (e.g., `gkg-server-deployed-indexer`)

### Environment variables
- `OTEL_EXPORTER_OTLP_ENDPOINT` — OTLP endpoint (e.g., `http://localhost:4317` or `http://localhost:4318`)
- `OTEL_EXPORTER_OTLP_PROTOCOL` — `grpc` or `http` (defaults to `grpc`)

### Start stack and view traces
1. Start observability stack (includes Jaeger):
   ```bash
   mise run observability:up
   ```
2. Run the server (example):
   ```bash
   # defaults to gRPC to localhost:4317
   cargo run --bin http-server-deployed -- \
     --secret-path /path/to/secret \
     --data-dir /tmp/gkg-data
   ```
   Or with custom OTEL settings:
   ```bash
   OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318 \
   OTEL_EXPORTER_OTLP_PROTOCOL=http \
   cargo run --bin http-server-deployed -- \
     --secret-path /path/to/secret \
     --data-dir /tmp/gkg-data
   ```
3. Generate a span:
   ```bash
   curl http://127.0.0.1:8080/health
   ```
4. Open Jaeger UI at `http://localhost:16686`:
   - Select service: `gkg-server-deployed-{mode}`
   - Click "Find Traces"

### Notes
- Jaeger runs in-memory locally; traces are lost on container restart.
- If Jaeger shows no services, make one request (e.g., `/health`) and refresh.


## Export OpenTelemetry traces to GitLab (OTLP HTTP)

By default, the server exports traces over OTLP gRPC to `http://localhost:4317`. If no collector is available, the server continues running without failing.

To export traces to GitLab Observability over OTLP HTTP, set:

- `OTEL_EXPORTER_OTLP_PROTOCOL=http`
- `OTEL_EXPORTER_OTLP_ENDPOINT=https://gitlab.com/api/v4/projects/69095239/observability/v1/traces` (must end with `v1/traces`)
- `OTEL_EXPORTER_OTLP_HEADERS=PRIVATE-TOKEN=<your-access-token>` (create a project access token with `api` scope)

Example:

```bash
OTEL_EXPORTER_OTLP_PROTOCOL=http \
OTEL_EXPORTER_OTLP_ENDPOINT=https://gitlab.com/api/v4/projects/69095239/observability/v1/traces \
OTEL_EXPORTER_OTLP_HEADERS="PRIVATE-TOKEN=<your-access-token>" \
cargo run --bin http-server-deployed -- --secret-path /path/to/secret
```

Troubleshooting (verbose logs):

```bash
RUST_LOG=opentelemetry=trace,opentelemetry_otlp=trace,opentelemetry_sdk=trace,opentelemetry_http=trace,reqwest=debug,hyper=debug,h2=trace \
RUST_BACKTRACE=full \
cargo run --bin http-server-deployed -- --secret-path /path/to/secret
```

References:
- GitLab distributed tracing docs: https://docs.gitlab.com/development/tracing/
