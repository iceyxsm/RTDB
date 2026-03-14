//! Quantization techniques for vector compression
//!
//! Provides production-grade quantization implementations:
//! - Product Quantization (PQ): 4-32x compression
//! - Binary Quantization: 32x compression
//! - Scalar Quantization: 4x compression
//! - Advanced Quantization (AQ): Superior reconstruction quality

pub mod product;
pub mod advanced;

pub use product::{ProductQuantizer, ProductQuantizerConfig, PQCodes};
pub use advanced::{AdvancedQuantizer, QuantizationConfig, QuantizationMethod};
