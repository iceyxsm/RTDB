//! Quantization techniques for vector compression

use crate::{Result, RTDBError, Vector};

/// Product Quantization
pub struct ProductQuantization {
    /// Number of subspaces
    num_subspaces: usize,
    /// Subvector dimension
    subvector_dim: usize,
    /// Codebooks (centroids) for each subspace
    codebooks: Vec<Vec<Vec<f32>>>,
}

impl ProductQuantization {
    /// Create PQ with M subspaces
    pub fn new(dim: usize, m: usize) -> Result<Self> {
        if dim % m != 0 {
            return Err(RTDBError::Index(
                format!("Dimension {} must be divisible by M {}", dim, m)
            ));
        }

        Ok(Self {
            num_subspaces: m,
            subvector_dim: dim / m,
            codebooks: Vec::with_capacity(m),
        })
    }

    /// Train codebooks on training vectors
    pub fn train(&mut self, vectors: &[Vector], k: usize) -> Result<()> {
        // Simple k-means for each subspace
        for m in 0..self.num_subspaces {
            let start = m * self.subvector_dim;
            let end = start + self.subvector_dim;

            // Extract subvectors
            let subvectors: Vec<Vec<f32>> = vectors
                .iter()
                .map(|v| v.data[start..end].to_vec())
                .collect();

            // K-means clustering
            let centroids = self.kmeans(&subvectors, k)?;
            self.codebooks.push(centroids);
        }

        Ok(())
    }

    /// Encode vector to codes
    pub fn encode(&self, vector: &Vector) -> Vec<u8> {
        let mut codes = Vec::with_capacity(self.num_subspaces);

        for m in 0..self.num_subspaces {
            let start = m * self.subvector_dim;
            let end = start + self.subvector_dim;
            let subvector = &vector.data[start..end];

            // Find nearest centroid
            let code = self.find_nearest(&self.codebooks[m], subvector);
            codes.push(code as u8);
        }

        codes
    }

    /// Decode codes to approximate vector
    pub fn decode(&self, codes: &[u8]) -> Vector {
        let mut data = Vec::with_capacity(self.num_subspaces * self.subvector_dim);

        for (m, &code) in codes.iter().enumerate() {
            if let Some(centroid) = self.codebooks[m].get(code as usize) {
                data.extend_from_slice(centroid);
            }
        }

        Vector::new(data)
    }

    /// Compute asymmetric distance (ADC)
    pub fn asymmetric_distance(&self, query: &Vector, codes: &[u8]) -> f32 {
        let mut distance = 0.0f32;

        for m in 0..self.num_subspaces {
            let start = m * self.subvector_dim;
            let end = start + self.subvector_dim;
            let subquery = &query.data[start..end];

            let code = codes[m] as usize;
            if let Some(centroid) = self.codebooks[m].get(code) {
                distance += Self::l2_distance_sq(subquery, centroid);
            }
        }

        distance.sqrt()
    }

    /// K-means clustering
    fn kmeans(&self, vectors: &[Vec<f32>], k: usize) -> Result<Vec<Vec<f32>>> {
        if vectors.is_empty() {
            return Ok(Vec::new());
        }

        // Simple k-means++ initialization
        let dim = vectors[0].len();
        let mut centroids: Vec<Vec<f32>> = Vec::with_capacity(k);
        
        // Pick first centroid randomly
        centroids.push(vectors[0].clone());

        // Pick remaining centroids
        for _ in 1..k {
            // Find farthest point
            let mut max_dist = 0.0;
            let mut farthest = 0;

            for (i, v) in vectors.iter().enumerate() {
                let dist = centroids.iter()
                    .map(|c| Self::l2_distance_sq(v, c))
                    .fold(f32::INFINITY, f32::min);
                
                if dist > max_dist {
                    max_dist = dist;
                    farthest = i;
                }
            }

            centroids.push(vectors[farthest].clone());
        }

        // Lloyd's iterations
        for _ in 0..20 {
            // Assign to clusters
            let mut assignments: Vec<Vec<usize>> = vec![Vec::new(); k];
            for (i, v) in vectors.iter().enumerate() {
                let nearest = self.find_nearest(&centroids, v);
                assignments[nearest].push(i);
            }

            // Update centroids
            for (j, cluster) in assignments.iter().enumerate() {
                if cluster.is_empty() {
                    continue;
                }

                let mut new_centroid = vec![0.0f32; dim];
                for &i in cluster {
                    for d in 0..dim {
                        new_centroid[d] += vectors[i][d];
                    }
                }
                for d in 0..dim {
                    new_centroid[d] /= cluster.len() as f32;
                }

                centroids[j] = new_centroid;
            }
        }

        Ok(centroids)
    }

    /// Find nearest centroid
    fn find_nearest(&self, centroids: &[Vec<f32>], vector: &[f32]) -> usize {
        let mut min_dist = f32::INFINITY;
        let mut nearest = 0;

        for (i, centroid) in centroids.iter().enumerate() {
            let dist = Self::l2_distance_sq(vector, centroid);
            if dist < min_dist {
                min_dist = dist;
                nearest = i;
            }
        }

        nearest
    }

    /// L2 distance squared
    fn l2_distance_sq(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum()
    }
}

/// Binary Quantization
pub struct BinaryQuantization {
    dimension: usize,
}

impl BinaryQuantization {
    /// Create BQ
    pub fn new(dim: usize) -> Self {
        Self { dimension: dim }
    }

    /// Encode vector to binary
    pub fn encode(&self, vector: &Vector) -> Vec<u8> {
        let num_bytes = (self.dimension + 7) / 8;
        let mut binary = vec![0u8; num_bytes];

        for (i, &val) in vector.data.iter().enumerate() {
            if val > 0.0 {
                binary[i / 8] |= 1 << (i % 8);
            }
        }

        binary
    }

    /// Compute Hamming distance
    pub fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
        a.iter().zip(b.iter())
            .map(|(x, y)| (x ^ y).count_ones())
            .sum()
    }
}

/// Scalar Quantization
pub struct ScalarQuantization {
    dimension: usize,
    bits: u8, // 4 or 8
    min: Vec<f32>,
    max: Vec<f32>,
}

impl ScalarQuantization {
    /// Create SQ
    pub fn new(dim: usize, bits: u8) -> Result<Self> {
        if bits != 4 && bits != 8 {
            return Err(RTDBError::Index("Bits must be 4 or 8".to_string()));
        }

        Ok(Self {
            dimension: dim,
            bits,
            min: vec![f32::INFINITY; dim],
            max: vec![f32::NEG_INFINITY; dim],
        })
    }

    /// Train quantization bounds
    pub fn train(&mut self, vectors: &[Vector]) {
        for v in vectors {
            for (i, &val) in v.data.iter().enumerate() {
                self.min[i] = self.min[i].min(val);
                self.max[i] = self.max[i].max(val);
            }
        }
    }

    /// Encode vector
    pub fn encode(&self, vector: &Vector) -> Vec<u8> {
        if self.bits == 8 {
            vector.data.iter().enumerate()
                .map(|(i, &v)| {
                    let norm = if self.max[i] > self.min[i] {
                        (v - self.min[i]) / (self.max[i] - self.min[i])
                    } else {
                        0.0
                    };
                    (norm * 255.0).clamp(0.0, 255.0) as u8
                })
                .collect()
        } else {
            // 4-bit
            let mut codes = vec![0u8; (self.dimension + 1) / 2];
            for (i, &v) in vector.data.iter().enumerate() {
                let norm = if self.max[i] > self.min[i] {
                    (v - self.min[i]) / (self.max[i] - self.min[i])
                } else {
                    0.0
                };
                let code = (norm * 15.0).clamp(0.0, 15.0) as u8;
                
                if i % 2 == 0 {
                    codes[i / 2] = code << 4;
                } else {
                    codes[i / 2] |= code;
                }
            }
            codes
        }
    }
}
