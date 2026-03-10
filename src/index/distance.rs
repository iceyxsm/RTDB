//! SIMD-optimized distance functions

use crate::Result;

/// Compute L2 distance (no SIMD - baseline)
pub fn l2_distance(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(crate::RTDBError::InvalidDimension {
            expected: a.len(),
            actual: b.len(),
        });
    }

    let sum: f32 = a.iter().zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum();

    Ok(sum.sqrt())
}

/// Compute dot product
pub fn dot_product(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(crate::RTDBError::InvalidDimension {
            expected: a.len(),
            actual: b.len(),
        });
    }

    Ok(a.iter().zip(b.iter())
        .map(|(x, y)| x * y)
        .sum())
}

/// Compute cosine similarity
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    let dot = dot_product(a, b)?;
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        Ok(0.0)
    } else {
        Ok(dot / (norm_a * norm_b))
    }
}

/// Batch L2 distance computation
pub fn batch_l2_distance(query: &[f32], vectors: &[Vec<f32>]) -> Result<Vec<f32>> {
    vectors.iter()
        .map(|v| l2_distance(query, v))
        .collect()
}

/// L2 distance squared (faster, no sqrt)
pub fn l2_distance_sq(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.len() != b.len() {
        return Err(crate::RTDBError::InvalidDimension {
            expected: a.len(),
            actual: b.len(),
        });
    }

    Ok(a.iter().zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_distance() {
        let a = [0.0, 0.0];
        let b = [3.0, 4.0];
        let dist = l2_distance(&a, &b).unwrap();
        assert!((dist - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_dot_product() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let dot = dot_product(&a, &b).unwrap();
        assert!((dot - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = [1.0, 0.0];
        let b = [0.0, 1.0];
        let sim = cosine_similarity(&a, &b).unwrap();
        assert!(sim.abs() < 1e-6);

        let c = [1.0, 0.0];
        let sim = cosine_similarity(&a, &c).unwrap();
        assert!((sim - 1.0).abs() < 1e-6);
    }
}
