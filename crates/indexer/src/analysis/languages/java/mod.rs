pub mod analyzer;
pub mod constant_resolver;
pub mod endpoint_extractor;
pub mod expression_resolver;
pub mod field_resolver;
pub mod java_file;
pub mod property_resolver;
pub mod service_call_extractor;
pub mod types;
pub mod utils;

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;

pub use analyzer::JavaAnalyzer;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::setup_java_reference_pipeline;
