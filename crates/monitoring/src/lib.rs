//! This crate provides observability infrastructure for the gkg application.
//!
//! It includes:
//! - Logging initialization with multiple modes (CLI, Server, etc.)
//! - OpenTelemetry tracing integration
//! - Prometheus metrics collection
//!
//! This crate is designed to be accessible from anywhere within the application
//! without creating circular dependencies.

pub mod logging;
pub mod metrics;
pub mod telemetry;

// Re-export commonly used types for convenience
pub use logging::{LogMode, LoggingGuards, init_logging};
