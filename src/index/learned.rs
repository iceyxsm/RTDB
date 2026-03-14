//! Learned Index implementation
//! 
//! Piecewise linear index for O(1) routing

use crate::Result;

/// Piecewise Linear Learned Index for efficient range queries and lookups.
/// 
/// Implements learned indexing using piecewise linear models to predict
/// data positions, reducing search complexity for sorted data access.
pub struct LearnedIndex {
    /// Number of linear model pieces for data approximation
    num_pieces: usize,
    /// Linear models (slope, intercept) for each piece
    models: Vec<(f64, f64)>,
    /// Boundaries between different linear model pieces
    boundaries: Vec<f64>,
}

impl LearnedIndex {
    /// Create new learned index
    pub fn new(num_pieces: usize) -> Self {
        Self {
            num_pieces,
            models: Vec::with_capacity(num_pieces),
            boundaries: Vec::with_capacity(num_pieces - 1),
        }
    }

    /// Train index on sorted keys
    pub fn train(&mut self, keys: &[f64]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }

        let piece_size = keys.len() / self.num_pieces;
        
        for i in 0..self.num_pieces {
            let start = i * piece_size;
            let end = if i == self.num_pieces - 1 {
                keys.len()
            } else {
                (i + 1) * piece_size
            };

            if start >= keys.len() {
                break;
            }

            let piece_keys = &keys[start..end.min(keys.len())];
            if piece_keys.len() < 2 {
                continue;
            }

            // Fit linear model y = mx + b
            // y is position, x is key value
            let n = piece_keys.len() as f64;
            let sum_x: f64 = piece_keys.iter().sum();
            let sum_y: f64 = (start..start + piece_keys.len())
                .map(|i| i as f64)
                .sum();
            let sum_xy: f64 = piece_keys
                .iter()
                .enumerate()
                .map(|(i, &x)| x * (start + i) as f64)
                .sum();
            let sum_x2: f64 = piece_keys.iter().map(|&x| x * x).sum();

            let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x);
            let intercept = (sum_y - slope * sum_x) / n;

            self.models.push((slope, intercept));

            if i < self.num_pieces - 1 && end < keys.len() {
                self.boundaries.push(keys[end]);
            }
        }

        Ok(())
    }

    /// Predict position for key
    pub fn predict(&self, key: f64) -> usize {
        let piece = self.find_piece(key);
        
        if let Some((slope, intercept)) = self.models.get(piece) {
            let pred = slope * key + intercept;
            pred.max(0.0) as usize
        } else {
            0
        }
    }

    /// Find piece for key
    fn find_piece(&self, key: f64) -> usize {
        // Binary search on boundaries
        let mut low = 0;
        let mut high = self.boundaries.len();

        while low < high {
            let mid = (low + high) / 2;
            if self.boundaries[mid] <= key {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        low.min(self.models.len().saturating_sub(1))
    }

    /// Get number of models
    pub fn num_models(&self) -> usize {
        self.models.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learned_index() {
        let mut index = LearnedIndex::new(4);
        
        // Create sorted keys (CDF-like)
        let keys: Vec<f64> = (0..1000).map(|i| i as f64 * 0.1).collect();
        
        index.train(&keys).unwrap();
        
        // Test predictions
        let pred = index.predict(50.0);
        assert!(pred > 400 && pred < 600, "Prediction {} out of range", pred);
    }
}
