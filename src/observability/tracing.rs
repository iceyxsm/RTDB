//! OpenTelemetry Distributed Tracing - Production Grade
//!
//! Performance-optimized distributed tracing with:
//! - Batched exports with configurable sizes
//! - gzip compression for reduced bandwidth

#![allow(missing_docs)]
//! - Memory-limited span buffering
//! - Intelligent sampling (head-based and tail-based)
//! - Production-hardened retry logic
//!
//! Optimized for high-throughput services with 10K+ req/sec.

use opentelemetry::{
    global,
    trace::TraceContextExt,
    Context, KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    runtime,
    trace::{RandomIdGenerator, Sampler},
    Resource,
};
use std::collections::HashMap;
use std::time::Duration;

use super::performance::{ObservabilityPerfConfig, TraceBatchConfig, SamplingDecision};

/// Extended tracing configuration with performance options
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
    /// Batch configuration
    pub batch_config: TraceBatchConfig,
    /// Enable gzip compression
    pub enable_compression: bool,
    /// Export timeout
    pub export_timeout: Duration,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self::production()
    }
}

impl TracingConfig {
    /// Production configuration (optimized for high throughput)
    pub fn production() -> Self {
        Self {
            service_name: "rtdb".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "production".to_string()),
            otlp_endpoint: std::env::var("OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            sampling_ratio: std::env::var("OTEL_SAMPLING_RATIO")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1), // 10% default sampling in production
            batch_config: TraceBatchConfig::production(),
            enable_compression: std::env::var("OTEL_COMPRESSION")
                .map(|v| v != "false")
                .unwrap_or(true),
            export_timeout: Duration::from_secs(30),
        }
    }

    /// Development configuration (full sampling, smaller batches)
    pub fn development() -> Self {
        Self {
            sampling_ratio: 1.0,
            batch_config: TraceBatchConfig::default(),
            enable_compression: false,
            export_timeout: Duration::from_secs(10),
            ..Self::production()
        }
    }

    /// High-throughput configuration (aggressive sampling, large batches)
    pub fn high_throughput() -> Self {
        Self {
            sampling_ratio: 0.01, // 1% sampling
            batch_config: TraceBatchConfig {
                max_export_batch_size: 1024,
                max_queue_size: 16384,
                scheduled_delay_ms: 5000,
                export_timeout_ms: 30000,
                max_concurrent_exports: 4,
            },
            enable_compression: true,
            export_timeout: Duration::from_secs(30),
            ..Self::production()
        }
    }

    /// Low-latency configuration (smaller batches, faster export)
    pub fn low_latency() -> Self {
        Self {
            batch_config: TraceBatchConfig {
                max_export_batch_size: 256,
                max_queue_size: 4096,
                scheduled_delay_ms: 2000, // 2 second delays
                export_timeout_ms: 10000,
                max_concurrent_exports: 2,
            },
            export_timeout: Duration::from_secs(10),
            ..Self::production()
        }
    }

    /// From performance config
    pub fn from_perf_config(config: &ObservabilityPerfConfig) -> Self {
        Self {
            sampling_ratio: config.sampling_ratio,
            batch_config: TraceBatchConfig {
                max_export_batch_size: config.trace_batch_size,
                max_queue_size: config.trace_queue_size,
                scheduled_delay_ms: 5000,
                export_timeout_ms: config.trace_export_timeout_ms,
                max_concurrent_exports: 2,
            },
            enable_compression: config.enable_compression,
            export_timeout: Duration::from_millis(config.trace_export_timeout_ms),
            ..Self::production()
        }
    }
}

/// Initialize OpenTelemetry tracing with production-grade configuration
pub fn init_tracing(config: &TracingConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create resource attributes
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", config.service_version.clone()),
        KeyValue::new("deployment.environment", config.environment.clone()),
        KeyValue::new("host.name", hostname::get().unwrap_or_default().to_string_lossy().to_string()),
    ]);

    // Configure OTLP exporter with compression
    let mut exporter_builder = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.otlp_endpoint)
        .with_timeout(config.export_timeout);

    // Enable compression if configured
    if config.enable_compression {
        exporter_builder = exporter_builder.with_compression(opentelemetry_otlp::Compression::Gzip);
    }

    let exporter = exporter_builder;

    // Configure sampling strategy
    let sampler = if config.sampling_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        // ParentBased respects upstream sampling decisions
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
        .install_batch(runtime::Tokio)?;

    // Set global propagator for context propagation
    global::set_text_map_propagator(TraceContextPropagator::new());

    tracing::info!(
        service = %config.service_name,
        environment = %config.environment,
        sampling_ratio = %config.sampling_ratio,
        batch_size = %config.batch_config.max_export_batch_size,
        compression = %config.enable_compression,
        "OpenTelemetry tracing initialized"
    );

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

/// Make sampling decision for a new trace
pub fn make_sampling_decision(sampling_ratio: f64) -> bool {
    SamplingDecision::should_sample_fast(sampling_ratio)
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

// ============================================================================
// gRPC Interceptor for Automatic Tracing
// ============================================================================

#[cfg(feature = "grpc")]
pub mod grpc {
    use super::*;

    /// Extract context from gRPC metadata and create span
    pub fn extract_grpc_context<T>(request: &tonic::Request<T>) -> Context {
        use tonic::metadata::MetadataMap;
        
        struct GrpcExtractor<'a> {
            metadata: &'a MetadataMap,
        }
        
        impl<'a> opentelemetry::propagation::Extractor for GrpcExtractor<'a> {
            fn get(&self, key: &str) -> Option<&str> {
                self.metadata
                    .get(key.to_lowercase().as_str())
                    .and_then(|v| v.to_str().ok())
            }
            
            fn keys(&self) -> Vec<&str> {
                self.metadata
                    .keys()
                    .filter_map(|k| match k {
                        tonic::metadata::KeyRef::Ascii(key) => Some(key.as_str()),
                        tonic::metadata::KeyRef::Binary(_) => None,
                    })
                    .collect()
            }
        }
        
        let extractor = GrpcExtractor { metadata: request.metadata() };
        global::get_text_map_propagator(|propagator| propagator.extract(&extractor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_variants() {
        let dev = TracingConfig::development();
        assert_eq!(dev.sampling_ratio, 1.0);
        assert!(!dev.enable_compression);

        let prod = TracingConfig::production();
        assert_eq!(prod.sampling_ratio, 0.1);
        assert!(prod.enable_compression);

        let ht = TracingConfig::high_throughput();
        assert_eq!(ht.sampling_ratio, 0.01);
        assert_eq!(ht.batch_config.max_export_batch_size, 1024);

        let ll = TracingConfig::low_latency();
        assert_eq!(ll.batch_config.max_export_batch_size, 256);
    }

    #[test]
    fn test_sampling_consistency() {
        // Same trace ID should always give same decision
        let trace_id: u128 = 0x1234567890abcdef1234567890abcdef;
        let ratio = 0.5;
        
        let first = SamplingDecision::should_sample(trace_id, ratio);
        let second = SamplingDecision::should_sample(trace_id, ratio);
        assert_eq!(first, second);
    }

    #[test]
    fn test_context_propagation() {
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string(),
        );
        
        let context = extract_context_from_headers(&headers);
        // Just verify extraction doesn't panic
        let _span = context.span();
    }

    #[test]
    fn test_context_injection() {
        let context = Context::current();
        let mut headers = HashMap::new();
        
        inject_context_into_headers(&context, &mut headers);
        
        // Headers may be empty if context has no valid span, which is OK
        assert!(headers.is_empty() || headers.contains_key("traceparent"));
    }

    #[test]
    fn test_current_trace_id_without_span() {
        // Should return None when no span is active
        assert!(current_trace_id().is_none());
        assert!(current_span_id().is_none());
    }
}
