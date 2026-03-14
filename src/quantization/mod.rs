//! Quantization Module
//!
//! This module provides various quantization techniques for vector compression
//! including advanced methods like additive and neural quantization.

pub mod advanced;

pub use advanced::{
    AdvancedQuantizer, 
    QuantizationConfig, 
    QuantizationMethod, 
    QuantizedVector,
    QuantizationError,
};