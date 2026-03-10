//! Distributed tracing support

use tracing::{info, span, Level};
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Initialize tracing with JSON format for production
pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}

/// Initialize tracing with JSON format
pub fn init_json_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_target(true)
                .with_span_list(true),
        )
        .init();
}

/// Create a span for collection operations
pub fn collection_span(collection: &str, operation: &str) -> tracing::Span {
    span!(
        Level::INFO,
        "collection_operation",
        collection = collection,
        operation = operation
    )
}

/// Create a span for query operations
pub fn query_span(collection: &str, query_type: &str) -> tracing::Span {
    span!(
        Level::DEBUG,
        "query",
        collection = collection,
        query_type = query_type
    )
}

/// Log collection operation
pub fn log_collection_op(collection: &str, operation: &str, details: &str) {
    info!(
        collection = collection,
        operation = operation,
        details = details,
        "Collection operation"
    );
}
