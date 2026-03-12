//! SIMD-optimized distance calculations
//!
//! This module provides high-performance distance calculations using
//! SIMD instructions (AVX-512, AVX2, NEON) based on industry best practices
//! from Qdrant, Milvus, and research papers.
//!
//! Performance targets (from industry benchmarks):
//! - Dot product 768D: ~35ns (VelesDB standard)
//! - Euclidean 1536D: ~70ns (Qdrant standard)
//! - 4-10x faster than naive implementations

use crate::{RTDBError, Result};

/// SIMD capability detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdCapability {
    /// No SIMD support (scalar fallback)
    Scalar,
    /// SSE2 support (128-bit)
    Sse2,
    /// AVX support (256-bit)
    Avx,
    /// AVX2 support (256-bit with FMA)
    Avx2,
    /// AVX-512 support (512-bit)
    Avx512,
    /// ARM NEON support (128-bit)
    Neon,
}

impl SimdCapability {
    /// Detect best available SIMD capability at runtime
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx512f") {
                SimdCapability::Avx512
            } else if is_x86_feature_detected!("avx2") {
                SimdCapability::Avx2
            } else if is_x86_feature_detected!("avx") {
                SimdCapability::Avx
            } else if is_x86_feature_detected!("sse2") {
                SimdCapability::Sse2
            } else {
                SimdCapability::Scalar
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("neon") {
                SimdCapability::Neon
            } else {
                SimdCapability::Scalar
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            SimdCapability::Scalar
        }
    }
}

/// Distance calculator with SIMD optimization
pub struct DistanceCalculator {
    capability: SimdCapability,
}

impl Default for DistanceCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl DistanceCalculator {
    /// Create new calculator with auto-detected SIMD capability
    pub fn new() -> Self {
        Self {
            capability: SimdCapability::detect(),
        }
    }

    /// Create with specific capability (for testing)
    pub fn with_capability(capability: SimdCapability) -> Self {
        Self { capability }
    }

    /// Get current SIMD capability
    pub fn capability(&self) -> SimdCapability {
        self.capability
    }

    /// Calculate dot product (inner product)
    /// Higher is better (similarity)
    pub fn dot_product(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(RTDBError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }

        let result = match self.capability {
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx512 => unsafe { dot_product_avx512(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx2 => unsafe { dot_product_avx2(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx | SimdCapability::Sse2 => unsafe { dot_product_sse(a, b) },
            #[cfg(target_arch = "aarch64")]
            SimdCapability::Neon => unsafe { dot_product_neon(a, b) },
            _ => dot_product_scalar(a, b),
        };

        Ok(result)
    }

    /// Calculate Euclidean distance (L2)
    /// Lower is better (distance)
    pub fn euclidean(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(RTDBError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }

        let result = match self.capability {
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx512 => unsafe { euclidean_avx512(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx2 => unsafe { euclidean_avx2(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx | SimdCapability::Sse2 => unsafe { euclidean_sse(a, b) },
            #[cfg(target_arch = "aarch64")]
            SimdCapability::Neon => unsafe { euclidean_neon(a, b) },
            _ => euclidean_scalar(a, b),
        };

        Ok(result.sqrt())
    }

    /// Calculate cosine similarity
    /// Higher is better (1.0 = identical, 0.0 = orthogonal)
    pub fn cosine(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(RTDBError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }

        let result = match self.capability {
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx512 => unsafe { cosine_avx512(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx2 => unsafe { cosine_avx2(a, b) },
            _ => cosine_scalar(a, b),
        };

        Ok(result)
    }

    /// Calculate Manhattan distance (L1)
    /// Lower is better
    pub fn manhattan(&self, a: &[f32], b: &[f32]) -> Result<f32> {
        if a.len() != b.len() {
            return Err(RTDBError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }

        let result = match self.capability {
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx512 => unsafe { manhattan_avx512(a, b) },
            #[cfg(target_arch = "x86_64")]
            SimdCapability::Avx2 => unsafe { manhattan_avx2(a, b) },
            _ => manhattan_scalar(a, b),
        };

        Ok(result)
    }
}

// ==================== SCALAR FALLBACKS ====================

#[inline(always)]
fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[inline(always)]
fn euclidean_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y) * (x - y)).sum()
}

fn cosine_scalar(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[inline(always)]
fn manhattan_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum()
}

// ==================== x86_64 SIMD IMPLEMENTATIONS ====================

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// AVX-512 dot product: 16 floats per iteration
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn dot_product_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm512_setzero_ps();
    let mut i = 0;

    // Process 16 elements at a time
    while i + 16 <= len {
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        sum = _mm512_fmadd_ps(va, vb, sum);
        i += 16;
    }

    // Horizontal sum
    let mut result = _mm512_reduce_add_ps(sum);

    // Handle remaining elements
    for j in i..len {
        result += a[j] * b[j];
    }

    result
}

/// AVX2 dot product: 8 floats per iteration with FMA
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();
    let mut i = 0;

    // Process 8 elements at a time
    while i + 8 <= len {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        sum = _mm256_fmadd_ps(va, vb, sum);
        i += 8;
    }

    // Horizontal sum using hadd
    let mut result = hsum256_ps(sum);

    // Handle remaining elements
    for j in i..len {
        result += a[j] * b[j];
    }

    result
}

/// SSE dot product: 4 floats per iteration
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn dot_product_sse(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm_setzero_ps();
    let mut i = 0;

    // Process 4 elements at a time
    while i + 4 <= len {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let vb = _mm_loadu_ps(b.as_ptr().add(i));
        sum = _mm_add_ps(sum, _mm_mul_ps(va, vb));
        i += 4;
    }

    // Horizontal sum
    let mut result = hsum128_ps(sum);

    // Handle remaining elements
    for j in i..len {
        result += a[j] * b[j];
    }

    result
}

/// AVX-512 Euclidean distance
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn euclidean_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm512_setzero_ps();
    let mut i = 0;

    while i + 16 <= len {
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        let diff = _mm512_sub_ps(va, vb);
        sum = _mm512_fmadd_ps(diff, diff, sum);
        i += 16;
    }

    let mut result = _mm512_reduce_add_ps(sum);

    for j in i..len {
        let diff = a[j] - b[j];
        result += diff * diff;
    }

    result
}

/// AVX2 Euclidean distance
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn euclidean_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();
    let mut i = 0;

    while i + 8 <= len {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let diff = _mm256_sub_ps(va, vb);
        sum = _mm256_fmadd_ps(diff, diff, sum);
        i += 8;
    }

    let mut result = hsum256_ps(sum);

    for j in i..len {
        let diff = a[j] - b[j];
        result += diff * diff;
    }

    result
}

/// SSE Euclidean distance
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn euclidean_sse(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm_setzero_ps();
    let mut i = 0;

    while i + 4 <= len {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let vb = _mm_loadu_ps(b.as_ptr().add(i));
        let diff = _mm_sub_ps(va, vb);
        sum = _mm_add_ps(sum, _mm_mul_ps(diff, diff));
        i += 4;
    }

    let mut result = hsum128_ps(sum);

    for j in i..len {
        let diff = a[j] - b[j];
        result += diff * diff;
    }

    result
}

/// AVX-512 Cosine similarity
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn cosine_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot = _mm512_setzero_ps();
    let mut norm_a = _mm512_setzero_ps();
    let mut norm_b = _mm512_setzero_ps();
    let mut i = 0;

    while i + 16 <= len {
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        
        dot = _mm512_fmadd_ps(va, vb, dot);
        norm_a = _mm512_fmadd_ps(va, va, norm_a);
        norm_b = _mm512_fmadd_ps(vb, vb, norm_b);
        
        i += 16;
    }

    let mut dot_result = _mm512_reduce_add_ps(dot);
    let mut norm_a_result = _mm512_reduce_add_ps(norm_a);
    let mut norm_b_result = _mm512_reduce_add_ps(norm_b);

    // Handle remaining elements
    for j in i..len {
        dot_result += a[j] * b[j];
        norm_a_result += a[j] * a[j];
        norm_b_result += b[j] * b[j];
    }

    let norm = norm_a_result.sqrt() * norm_b_result.sqrt();
    if norm == 0.0 {
        0.0
    } else {
        dot_result / norm
    }
}

/// AVX2 Cosine similarity
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn cosine_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut dot = _mm256_setzero_ps();
    let mut norm_a = _mm256_setzero_ps();
    let mut norm_b = _mm256_setzero_ps();
    let mut i = 0;

    while i + 8 <= len {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        
        dot = _mm256_fmadd_ps(va, vb, dot);
        norm_a = _mm256_fmadd_ps(va, va, norm_a);
        norm_b = _mm256_fmadd_ps(vb, vb, norm_b);
        
        i += 8;
    }

    let mut dot_result = hsum256_ps(dot);
    let mut norm_a_result = hsum256_ps(norm_a);
    let mut norm_b_result = hsum256_ps(norm_b);

    for j in i..len {
        dot_result += a[j] * b[j];
        norm_a_result += a[j] * a[j];
        norm_b_result += b[j] * b[j];
    }

    let norm = norm_a_result.sqrt() * norm_b_result.sqrt();
    if norm == 0.0 {
        0.0
    } else {
        dot_result / norm
    }
}

/// AVX-512 Manhattan distance
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn manhattan_avx512(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm512_setzero_ps();
    let mut i = 0;

    while i + 16 <= len {
        let va = _mm512_loadu_ps(a.as_ptr().add(i));
        let vb = _mm512_loadu_ps(b.as_ptr().add(i));
        let diff = _mm512_sub_ps(va, vb);
        sum = _mm512_add_ps(sum, _mm512_abs_ps(diff));
        i += 16;
    }

    let mut result = _mm512_reduce_add_ps(sum);

    for j in i..len {
        result += (a[j] - b[j]).abs();
    }

    result
}

/// AVX2 Manhattan distance
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn manhattan_avx2(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = _mm256_setzero_ps();
    let mut i = 0;

    while i + 8 <= len {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let diff = _mm256_sub_ps(va, vb);
        // Absolute value: abs(x) = max(x, -x)
        let abs_diff = _mm256_max_ps(diff, _mm256_sub_ps(_mm256_setzero_ps(), diff));
        sum = _mm256_add_ps(sum, abs_diff);
        i += 8;
    }

    let mut result = hsum256_ps(sum);

    for j in i..len {
        result += (a[j] - b[j]).abs();
    }

    result
}

// ==================== Helper Functions ====================

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn hsum256_ps(v: __m256) -> f32 {
    // [a, b, c, d, e, f, g, h] -> a+b+c+d+e+f+g+h
    let sum1 = _mm256_hadd_ps(v, v);
    let sum2 = _mm256_hadd_ps(sum1, sum1);
    let sum3 = _mm_add_ps(
        _mm256_castps256_ps128(sum2),
        _mm256_extractf128_ps(sum2, 1)
    );
    _mm_cvtss_f32(sum3)
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn hsum128_ps(v: __m128) -> f32 {
    let shuf = _mm_movehdup_ps(v);
    let sums = _mm_add_ps(v, shuf);
    let sum = _mm_add_ss(sums, _mm_movehl_ps(sums, sums));
    _mm_cvtss_f32(sum)
}

// ==================== ARM NEON IMPLEMENTATIONS ====================

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_product_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = vdupq_n_f32(0.0);
    let mut i = 0;

    // Process 4 elements at a time
    while i + 4 <= len {
        let va = vld1q_f32(a.as_ptr().add(i));
        let vb = vld1q_f32(b.as_ptr().add(i));
        sum = vfmaq_f32(sum, va, vb);
        i += 4;
    }

    // Horizontal sum
    let mut result = vaddvq_f32(sum);

    // Handle remaining elements
    for j in i..len {
        result += a[j] * b[j];
    }

    result
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn euclidean_neon(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let mut sum = vdupq_n_f32(0.0);
    let mut i = 0;

    while i + 4 <= len {
        let va = vld1q_f32(a.as_ptr().add(i));
        let vb = vld1q_f32(b.as_ptr().add(i));
        let diff = vsubq_f32(va, vb);
        sum = vfmaq_f32(sum, diff, diff);
        i += 4;
    }

    let mut result = vaddvq_f32(sum);

    for j in i..len {
        let diff = a[j] - b[j];
        result += diff * diff;
    }

    result
}

// ==================== TESTS ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_product() {
        let calc = DistanceCalculator::new();
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![5.0, 6.0, 7.0, 8.0];
        
        let result = calc.dot_product(&a, &b).unwrap();
        assert!((result - 70.0).abs() < 1e-5);
    }

    #[test]
    fn test_euclidean() {
        let calc = DistanceCalculator::new();
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        
        let result = calc.euclidean(&a, &b).unwrap();
        let expected: f32 = ((1.0f32-4.0).powi(2) + (2.0f32-5.0).powi(2) + (3.0f32-6.0).powi(2)).sqrt();
        assert!((result - expected).abs() < 1e-5);
    }

    #[test]
    fn test_cosine() {
        let calc = DistanceCalculator::new();
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        
        let result = calc.cosine(&a, &b).unwrap();
        assert!((result - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_large_vectors() {
        let calc = DistanceCalculator::new();
        let a = vec![1.0; 1536]; // OpenAI embedding dimension
        let b = vec![2.0; 1536];
        
        let result = calc.dot_product(&a, &b).unwrap();
        assert!((result - 3072.0).abs() < 1e-3);
    }
}
