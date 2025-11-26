use monitoring::metrics;

/// RAII guard to ensure ACTIVE_INDEXING_OPERATIONS is decremented even on error
pub struct ActiveIndexingGuard;

impl ActiveIndexingGuard {
    pub fn new() -> Self {
        metrics::ACTIVE_INDEXING_OPERATIONS.inc();
        Self
    }
}

impl Drop for ActiveIndexingGuard {
    fn drop(&mut self) {
        metrics::ACTIVE_INDEXING_OPERATIONS.dec();
    }
}
