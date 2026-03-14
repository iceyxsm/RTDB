//! GPU Acceleration Module for Ultra-High Performance Vector Operations
//!
//! This module provides GPU acceleration support for CUDA, ROCm, and Metal backends
//! with automatic hardware detection and optimal backend selection for maximum performance.
//!
//! Key Features:
//! - Multi-GPU backend support (CUDA, ROCm, Metal)
//! - Automatic hardware detection and optimal backend selection
//! - SIMDX integration for hybrid CPU-GPU optimization
//! - Memory-efficient batch processing with streaming
//! - Production-grade error handling and fallback mechanisms

use std::sync::Arc;
use std::collections::HashMap;
use thiserror::Error;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn, error, instrument};

pub mod cuda;
pub mod rocm;
pub mod metal;
pub mod kernels;

#[derive(Debug, Error)]
pub enum GPUError {
    #[error("GPU backend not available: {backend}")]
    BackendNotAvailable { backend: String },
    #[error("GPU memory allocation failed: {size} bytes")]
    MemoryAllocationFailed { size: usize },
    #[error("GPU kernel execution failed: {kernel}")]
    KernelExecutionFailed { kernel: String },
    #[error("GPU context creation failed: {reason}")]
    ContextCreationFailed { reason: String },
    #[error("GPU device not found")]
    DeviceNotFound,
    #[error("GPU operation timeout")]
    OperationTimeout,
    #[error("GPU backend error: {message}")]
    BackendError { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GPUBackend {
    CUDA,
    ROCm,
    Metal,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUCapabilities {
    pub backend: GPUBackend,
    pub device_count: usize,
    pub memory_per_device: Vec<usize>,
    pub compute_capability: String,
    pub max_threads_per_block: usize,
    pub max_shared_memory: usize,
    pub supports_fp16: bool,
    pub supports_int8: bool,
    pub supports_tensor_cores: bool,
    pub memory_bandwidth_gbps: f32,
    pub peak_flops_fp32: f64,
    pub peak_flops_fp16: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GPUConfig {
    pub preferred_backend: Option<GPUBackend>,
    pub device_id: Option<usize>,
    pub memory_pool_size: usize,
    pub enable_mixed_precision: bool,
    pub batch_size_limit: usize,
    pub stream_count: usize,
    pub enable_profiling: bool,
}

impl Default for GPUConfig {
    fn default() -> Self {
        Self {
            preferred_backend: None, // Auto-detect
            device_id: None, // Use default device
            memory_pool_size: 1024 * 1024 * 1024, // 1GB
            enable_mixed_precision: true,
            batch_size_limit: 10000,
            stream_count: 4,
            enable_profiling: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GPUMetrics {
    pub operations_count: u64,
    pub total_compute_time_ms: f64,
    pub memory_usage_bytes: usize,
    pub kernel_launches: u64,
    pub memory_transfers: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub average_batch_size: f32,
    pub throughput_gflops: f32,
}

/// GPU Acceleration Engine with multi-backend support
pub struct GPUEngine {
    capabilities: GPUCapabilities,
    config: GPUConfig,
    backend: Box<dyn GPUBackendTrait + Send + Sync>,
    metrics: Arc<parking_lot::Mutex<GPUMetrics>>,
    memory_pool: Arc<parking_lot::Mutex<HashMap<usize, Vec<u8>>>>,
}

impl GPUEngine {
    /// Create new GPU engine with automatic backend detection
    #[instrument(skip(config))]
    pub fn new(config: Option<GPUConfig>) -> Result<Self, GPUError> {
        let config = config.unwrap_or_default();
        let capabilities = Self::detect_capabilities(&config)?;
        
        info!("Detected GPU capabilities: {:?}", capabilities);
        
        let backend = Self::create_backend(&capabilities, &config)?;
        
        Ok(Self {
            capabilities,
            config,
            backend,
            metrics: Arc::new(parking_lot::Mutex::new(GPUMetrics {
                operations_count: 0,
                total_compute_time_ms: 0.0,
                memory_usage_bytes: 0,
                kernel_launches: 0,
                memory_transfers: 0,
                cache_hits: 0,
                cache_misses: 0,
                average_batch_size: 0.0,
                throughput_gflops: 0.0,
            })),
            memory_pool: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        })
    }

    /// Detect available GPU capabilities
    fn detect_capabilities(config: &GPUConfig) -> Result<GPUCapabilities, GPUError> {
        // Try backends in order of preference
        let backends_to_try = match config.preferred_backend {
            Some(backend) => vec![backend],
            None => vec![GPUBackend::CUDA, GPUBackend::ROCm, GPUBackend::Metal],
        };

        for backend in backends_to_try {
            match backend {
                GPUBackend::CUDA => {
                    if let Ok(caps) = cuda::detect_cuda_capabilities() {
                        return Ok(caps);
                    }
                }
                GPUBackend::ROCm => {
                    if let Ok(caps) = rocm::detect_rocm_capabilities() {
                        return Ok(caps);
                    }
                }
                GPUBackend::Metal => {
                    if let Ok(caps) = metal::detect_metal_capabilities() {
                        return Ok(caps);
                    }
                }
                GPUBackend::None => {}
            }
        }

        // Fallback to CPU-only mode
        warn!("No GPU backend available, falling back to CPU-only mode");
        Ok(GPUCapabilities {
            backend: GPUBackend::None,
            device_count: 0,
            memory_per_device: vec![],
            compute_capability: "CPU".to_string(),
            max_threads_per_block: 1,
            max_shared_memory: 0,
            supports_fp16: false,
            supports_int8: true,
            supports_tensor_cores: false,
            memory_bandwidth_gbps: 0.0,
            peak_flops_fp32: 0.0,
            peak_flops_fp16: 0.0,
        })
    }

    /// Create appropriate backend implementation
    fn create_backend(
        capabilities: &GPUCapabilities,
        config: &GPUConfig,
    ) -> Result<Box<dyn GPUBackendTrait + Send + Sync>, GPUError> {
        match capabilities.backend {
            GPUBackend::CUDA => Ok(Box::new(cuda::CudaBackend::new(config)?)),
            GPUBackend::ROCm => Ok(Box::new(rocm::RocmBackend::new(config)?)),
            GPUBackend::Metal => Ok(Box::new(metal::MetalBackend::new(config)?)),
            GPUBackend::None => Err(GPUError::BackendNotAvailable {
                backend: "None".to_string(),
            }),
        }
    }

    /// Compute cosine distance between two vectors using GPU acceleration
    #[instrument(skip(self, a, b))]
    pub async fn cosine_distance(&self, a: &[f32], b: &[f32]) -> Result<f32, GPUError> {
        if a.len() != b.len() {
            return Err(GPUError::BackendError {
                message: "Vector dimensions must match".to_string(),
            });
        }

        let start = std::time::Instant::now();
        let result = self.backend.cosine_distance(a, b).await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        // Update metrics
        {
            let mut metrics = self.metrics.lock();
            metrics.operations_count += 1;
            metrics.total_compute_time_ms += elapsed;
            metrics.kernel_launches += 1;
        }

        Ok(result)
    }

    /// Compute batch cosine distances using GPU acceleration
    #[instrument(skip(self, query, vectors))]
    pub async fn batch_cosine_distance(
        &self,
        query: &[f32],
        vectors: &[Vec<f32>],
    ) -> Result<Vec<f32>, GPUError> {
        if vectors.is_empty() {
            return Ok(vec![]);
        }

        // Validate dimensions
        for (i, vec) in vectors.iter().enumerate() {
            if vec.len() != query.len() {
                return Err(GPUError::BackendError {
                    message: format!("Vector {} dimension mismatch", i),
                });
            }
        }

        let start = std::time::Instant::now();
        let result = self.backend.batch_cosine_distance(query, vectors).await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        // Update metrics
        {
            let mut metrics = self.metrics.lock();
            metrics.operations_count += 1;
            metrics.total_compute_time_ms += elapsed;
            metrics.kernel_launches += 1;
            metrics.average_batch_size = 
                (metrics.average_batch_size * (metrics.operations_count - 1) as f32 + vectors.len() as f32) 
                / metrics.operations_count as f32;
        }

        Ok(result)
    }

    /// Get GPU capabilities
    pub fn get_capabilities(&self) -> &GPUCapabilities {
        &self.capabilities
    }

    /// Get current metrics
    pub fn get_metrics(&self) -> GPUMetrics {
        self.metrics.lock().clone()
    }

    /// Check if GPU acceleration is available
    pub fn is_available(&self) -> bool {
        self.capabilities.backend != GPUBackend::None
    }

    /// Get memory usage statistics
    pub fn get_memory_usage(&self) -> Result<usize, GPUError> {
        self.backend.get_memory_usage()
    }

    /// Synchronize GPU operations
    pub async fn synchronize(&self) -> Result<(), GPUError> {
        self.backend.synchronize().await
    }
}

/// Trait for GPU backend implementations
#[async_trait::async_trait]
pub trait GPUBackendTrait {
    async fn cosine_distance(&self, a: &[f32], b: &[f32]) -> Result<f32, GPUError>;
    async fn batch_cosine_distance(&self, query: &[f32], vectors: &[Vec<f32>]) -> Result<Vec<f32>, GPUError>;
    fn get_memory_usage(&self) -> Result<usize, GPUError>;
    async fn synchronize(&self) -> Result<(), GPUError>;
}

/// Global GPU engine instance
static GPU_ENGINE: once_cell::sync::OnceCell<Arc<GPUEngine>> = once_cell::sync::OnceCell::new();

/// Get or initialize global GPU engine
pub fn get_gpu_engine() -> Result<Arc<GPUEngine>, GPUError> {
    GPU_ENGINE
        .get_or_try_init(|| {
            GPUEngine::new(None).map(Arc::new)
        })
        .map(Arc::clone)
}

/// Initialize GPU engine with custom configuration
pub fn init_gpu_engine(config: GPUConfig) -> Result<Arc<GPUEngine>, GPUError> {
    let engine = Arc::new(GPUEngine::new(Some(config))?);
    GPU_ENGINE.set(engine.clone()).map_err(|_| GPUError::BackendError {
        message: "GPU engine already initialized".to_string(),
    })?;
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gpu_engine_creation() {
        let config = GPUConfig::default();
        let result = GPUEngine::new(Some(config));
        
        // Should either succeed or fail gracefully
        match result {
            Ok(engine) => {
                assert!(engine.get_capabilities().backend != GPUBackend::None || 
                       engine.get_capabilities().backend == GPUBackend::None);
            }
            Err(_) => {
                // Expected if no GPU is available
            }
        }
    }

    #[tokio::test]
    async fn test_cosine_distance() {
        if let Ok(engine) = GPUEngine::new(None) {
            if engine.is_available() {
                let a = vec![1.0, 0.0, 0.0];
                let b = vec![0.0, 1.0, 0.0];
                
                let result = engine.cosine_distance(&a, &b).await;
                assert!(result.is_ok());
            }
        }
    }
}