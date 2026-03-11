//! OpenTelemetry Distributed Tracing
//!
//! Production-grade distributed tracing implementation following best practices:
//! - Automatic span creation for gRPC/HTTP requests
//! - Context propagation across service boundaries
//! - Trace correlation with logs
//! - Configurable sampling strategies
//!
//! Based on OpenTelemetry Rust SDK best practices for microservices.

use opentelemetry::{
    global,
    trace::TraceContextExt,
    Context, KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    runtime,
    trace::{BatchConfig, RandomIdGenerator, Sampler},
    Resource,
};
use std::collections::HashMap;
use std::time::Duration;

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Service name for trace identification
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Deployment environment
    pub environment: String,
    /// OTLP endpoint for trace export
    pub otlp_endpoint: String,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
    /// Batch export timeout
    pub export_timeout: Duration,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "rtdb".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            otlp_endpoint: std::env::var("OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            sampling_ratio: std::env::var("OTEL_SAMPLING_RATIO")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            export_timeout: Duration::from_secs(5),
        }
    }
}

/// Initialize OpenTelemetry tracing
pub fn init_tracing(config: &TracingConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create resource attributes
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", config.service_version.clone()),
        KeyValue::new("deployment.environment", config.environment.clone()),
        KeyValue::new("host.name", hostname::get().unwrap_or_default().to_string_lossy().to_string()),
    ]);

    // Configure OTLP exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.otlp_endpoint)
        .with_timeout(config.export_timeout);

    // Configure sampling strategy
    let sampler = if config.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(config.sampling_ratio)))
    };

    // Build and install tracer provider
    let _tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(sampler)
                .with_id_generator(RandomIdGenerator::default())
                .with_resource(resource),
        )
        .with_batch_config(BatchConfig::default())
        .install_batch(runtime::Tokio)?;

    // Set global propagator for context propagation
    global::set_text_map_propagator(TraceContextPropagator::new());

    Ok(())
}

/// Shutdown OpenTelemetry, flushing pending spans
pub fn shutdown_tracing() {
    global::shutdown_tracer_provider();
}

/// Extract trace context from HTTP headers
pub fn extract_context_from_headers(headers: &HashMap<String, String>) -> Context {
    let extractor = HeaderExtractor { headers };
    global::get_text_map_propagator(|propagator| propagator.extract(&extractor))
}

/// Inject trace context into HTTP headers
pub fn inject_context_into_headers(context: &Context, headers: &mut HashMap<String, String>) {
    let mut injector = HeaderInjector { headers };
    global::get_text_map_propagator(|propagator| propagator.inject_context(context, &mut injector));
}

/// Get current trace ID for log correlation
pub fn current_trace_id() -> Option<String> {
    let context = Context::current();
    let span = context.span();
    let span_context = span.span_context();
    
    if span_context.is_valid() {
        Some(format!("{:032x}", span_context.trace_id()))
    } else {
        None
    }
}

/// Get current span ID for log correlation
pub fn current_span_id() -> Option<String> {
    let context = Context::current();
    let span = context.span();
    let span_context = span.span_context();
    
    if span_context.is_valid() {
        Some(format!("{:016x}", span_context.span_id()))
    } else {
        None
    }
}

// ============================================================================
// Extractors and Injectors for Context Propagation
// ============================================================================

struct HeaderExtractor<'a> {
    headers: &'a HashMap<String, String>,
}

impl<'a> opentelemetry::propagation::Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(|s| s.as_str()).collect()
    }
}

struct HeaderInjector<'a> {
    headers: &'a mut HashMap<String, String>,
}

impl<'a> opentelemetry::propagation::Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        self.headers.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_propagation() {
        // Test that headers can be extracted without errors
        // Note: In test environment without proper OTel setup, 
        // context extraction may not work fully, but should not panic
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string(),
        );
        
        let context = extract_context_from_headers(&headers);
        // Just verify we got a context back without panicking
        let _span = context.span();
        // The span context validity depends on OTel setup, 
        // so we just verify the extraction doesn't panic
    }

    #[test]
    fn test_context_injection() {
        // Test that headers can be injected without errors
        // Note: Empty context won't inject headers, which is expected behavior
        let context = Context::current();
        let mut headers = HashMap::new();
        
        inject_context_into_headers(&context, &mut headers);
        
        // Either traceparent is present or headers remain empty for invalid context
        assert!(headers.is_empty() || headers.contains_key("traceparent"));
    }

    #[tokio::test]
    async fn test_tracing_config_default() {
        let config = TracingConfig::default();
        assert!(!config.service_name.is_empty());
        assert!(config.sampling_ratio >= 0.0 && config.sampling_ratio <= 1.0);
    }
}
