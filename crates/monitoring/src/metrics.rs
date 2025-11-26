use lazy_static::lazy_static;
use prometheus::{
    Counter, CounterVec, Gauge, Histogram, HistogramVec, register_counter, register_counter_vec,
    register_gauge, register_histogram, register_histogram_vec,
};

lazy_static! {
    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "gkg_http_request_duration_seconds",
        "HTTP request latencies in seconds",
        &["method", "path"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap();
    pub static ref HTTP_RESPONSES_TOTAL: CounterVec = register_counter_vec!(
        "gkg_http_responses_total",
        "Total number of HTTP responses by status code",
        &["method", "path", "status"]
    )
    .unwrap();

    // Indexing Metrics
    pub static ref INDEXING_DURATION_SECONDS: Histogram = register_histogram!(
        "gkg_indexing_duration_seconds",
        "Overall indexing duration",
        vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0]
    )
    .unwrap();

    pub static ref INDEXING_PHASE_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "gkg_indexing_phase_duration_seconds",
        "Duration of each indexing phase",
        &["phase"],
        vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 120.0]
    )
    .unwrap();

    pub static ref FILES_PROCESSED_TOTAL: CounterVec = register_counter_vec!(
        "gkg_files_processed_total",
        "Total files successfully processed by language",
        &["language"]
    )
    .unwrap();

    pub static ref FILES_ERRORED_TOTAL: CounterVec = register_counter_vec!(
        "gkg_files_errored_total",
        "Total files that errored during processing by language",
        &["language"]
    )
    .unwrap();

    pub static ref DEFINITIONS_CREATED_TOTAL: Counter = register_counter!(
        "gkg_definitions_created_total",
        "Total definitions indexed"
    )
    .unwrap();

    pub static ref RELATIONSHIPS_CREATED_TOTAL: Counter = register_counter!(
        "gkg_relationships_created_total",
        "Total relationships created"
    )
    .unwrap();

    pub static ref ACTIVE_INDEXING_OPERATIONS: Gauge = register_gauge!(
        "gkg_active_indexing_operations",
        "Number of currently active indexing operations"
    )
    .unwrap();

    // Database Metrics
    pub static ref DATABASE_QUERY_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "gkg_database_query_duration_seconds",
        "Database query execution duration by operation type",
        &["operation_type"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap();

    pub static ref DATABASE_QUERIES_TOTAL: CounterVec = register_counter_vec!(
        "gkg_database_queries_total",
        "Total number of database queries by status",
        &["status"]
    )
    .unwrap();

    pub static ref DATABASE_OPERATIONS_ACTIVE: Gauge = register_gauge!(
        "gkg_database_operations_active",
        "Number of currently active database operations"
    )
    .unwrap();
}
