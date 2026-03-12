//! Quantization techniques for vector compression
//!
//! Provides production-grade quantization implementations:
//! - Product Quantization (PQ): 4-32x compression
//! - Binary Quantization: 32x compression
//! - Scalar Quantization: 4x compression

pub mod product;

pub use product::{ProductQuantizer, ProductQuantizerConfig, PQCodes};
