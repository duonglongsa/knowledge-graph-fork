use monitoring::metrics;
use std::time::Instant;

/// RAII guard that automatically increments/decrements the active database operations gauge
/// and records query duration and status when dropped
pub struct ActiveDatabaseOperationGuard {
    start: Instant,
    operation_type: &'static str,
    success: Option<bool>,
}

impl ActiveDatabaseOperationGuard {
    pub fn new(operation_type: &'static str) -> Self {
        metrics::DATABASE_OPERATIONS_ACTIVE.inc();
        Self {
            start: Instant::now(),
            operation_type,
            success: None,
        }
    }

    pub fn set_success(&mut self, success: bool) {
        self.success = Some(success);
    }
}

impl Drop for ActiveDatabaseOperationGuard {
    fn drop(&mut self) {
        metrics::DATABASE_OPERATIONS_ACTIVE.dec();

        // Record metrics if success status was set
        if let Some(success) = self.success {
            let duration = self.start.elapsed().as_secs_f64();

            metrics::DATABASE_QUERY_DURATION_SECONDS
                .with_label_values(&[self.operation_type])
                .observe(duration);

            let status = if success { "success" } else { "error" };
            metrics::DATABASE_QUERIES_TOTAL
                .with_label_values(&[status])
                .inc();
        }
    }
}

/// Record a database query with duration and status
pub fn record_query(operation_type: &str, duration_seconds: f64, success: bool) {
    metrics::DATABASE_QUERY_DURATION_SECONDS
        .with_label_values(&[operation_type])
        .observe(duration_seconds);

    let status = if success { "success" } else { "error" };
    metrics::DATABASE_QUERIES_TOTAL
        .with_label_values(&[status])
        .inc();
}
