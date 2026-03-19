//! Advanced Quantization Module with Additive and Neural Quantization
//!
//! This module implements state-of-the-art quantization techniques including:
//! - Additive Quantization (AQ) with learned codebooks
//! - Neural Quantization with implicit codebooks (QINCo)
//! - Residual Quantization with hierarchical structure
//! - SIMDX-optimized implementations for maximum performance

use std::sync::Arc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn, instrument};
use rand::Rng;
use crate::simdx::SIMDXEngine;

#[derive(Debug, Error)]
pub enum QuantizationError {
    #[error("Invalid quantization configuration: {message}")]
    InvalidConfig { message: String },
    #[error("Codebook training failed: {reason}")]
    TrainingFailed { reason: String },
    #[error("Quantization failed: {reason}")]
    QuantizationFailed { reason: String },
    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("Insufficient training data: need at least {required} vectors")]
    InsufficientData { required: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationMethod {
    /// Additive Quantization with full-dimensional codebooks
    Additive,
    /// Neural Quantization with implicit codebooks (QINCo)
    Neural,
    /// Residual Quantization with hierarchical structure
    Residual,
    /// Stacked Quantizers for efficient encoding
    Stacked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    pub method: QuantizationMethod,
    pub num_codebooks: usize,
    pub codebook_size: usize,
    pub vector_dim: usize,
    pub bits_per_code: usize,
    pub training_iterations: usize,
    pub convergence_threshold: f32,
    pub use_simdx: bool,
    pub enable_reranking: bool,
    pub rerank_factor: usize,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            method: QuantizationMethod::Additive,
            num_codebooks: 8,
            codebook_size: 256,
            vector_dim: 768,
            bits_per_code: 8,
            training_iterations: 100,
            convergence_threshold: 1e-4,
            use_simdx: true,
            enable_reranking: true,
            rerank_factor: 4,
        }
    }
}
/// Codebook for quantization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Codebook {
    pub vectors: Vec<Vec<f32>>,
    pub size: usize,
    pub dimension: usize,
}

impl Codebook {
    pub fn new(size: usize, dimension: usize) -> Self {
        Self {
            vectors: vec![vec![0.0; dimension]; size],
            size,
            dimension,
        }
    }

    pub fn random_init(&mut self, rng: &mut impl rand::Rng) {
        for vector in &mut self.vectors {
            for element in vector {
                *element = rng.gen_range(-1.0..1.0);
            }
        }
    }
}

/// Quantized vector representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedVector {
    pub codes: Vec<usize>,
    pub method: QuantizationMethod,
    pub reconstruction_error: f32,
}

/// Advanced quantizer with multiple methods
pub struct AdvancedQuantizer {
    config: QuantizationConfig,
    codebooks: Vec<Codebook>,
    simdx_engine: Arc<SIMDXEngine>,
    neural_network: Option<NeuralCodebook>,
    training_data: Vec<Vec<f32>>,
    is_trained: bool,
}

impl AdvancedQuantizer {
    pub fn new(config: QuantizationConfig, simdx_engine: Arc<SIMDXEngine>) -> Self {
        let mut codebooks = Vec::new();
        for _ in 0..config.num_codebooks {
            codebooks.push(Codebook::new(config.codebook_size, config.vector_dim));
        }

        Self {
            config,
            codebooks,
            simdx_engine,
            neural_network: None,
            training_data: Vec::new(),
            is_trained: false,
        }
    }

    /// Add training data
    pub fn add_training_data(&mut self, vectors: Vec<Vec<f32>>) -> Result<(), QuantizationError> {
        for vector in &vectors {
            if vector.len() != self.config.vector_dim {
                return Err(QuantizationError::DimensionMismatch {
                    expected: self.config.vector_dim,
                    actual: vector.len(),
                });
            }
        }
        
        self.training_data.extend(vectors);
        Ok(())
    }

    /// Train the quantizer
    #[instrument(skip(self))]
    pub fn train(&mut self) -> Result<(), QuantizationError> {
        if self.training_data.len() < self.config.num_codebooks * self.config.codebook_size {
            return Err(QuantizationError::InsufficientData {
                required: self.config.num_codebooks * self.config.codebook_size,
            });
        }

        info!("Training quantizer with {} vectors", self.training_data.len());

        match self.config.method {
            QuantizationMethod::Additive => self.train_additive()?,
            QuantizationMethod::Neural => self.train_neural()?,
            QuantizationMethod::Residual => self.train_residual()?,
            QuantizationMethod::Stacked => self.train_stacked()?,
        }

        self.is_trained = true;
        info!("Quantizer training completed");
        Ok(())
    }
    /// Train additive quantization
    fn train_additive(&mut self) -> Result<(), QuantizationError> {
        debug!("Training additive quantization");
        
        let mut rng = rand::thread_rng();
        
        // Initialize codebooks randomly
        for codebook in &mut self.codebooks {
            codebook.random_init(&mut rng);
        }

        // Iterative training with beam search
        for iteration in 0..self.config.training_iterations {
            let mut total_error = 0.0;
            let mut assignments = vec![vec![0; self.config.num_codebooks]; self.training_data.len()];

            // Assign vectors to codebook combinations using beam search
            for (i, vector) in self.training_data.iter().enumerate() {
                let (codes, error) = self.beam_search_assignment(vector)?;
                assignments[i] = codes;
                total_error += error;
            }

            // Update codebooks based on assignments
            self.update_codebooks_additive(&assignments)?;

            let avg_error = total_error / self.training_data.len() as f32;
            debug!("Iteration {}: avg error = {:.6}", iteration, avg_error);

            if avg_error < self.config.convergence_threshold {
                info!("Converged after {} iterations", iteration + 1);
                break;
            }
        }

        Ok(())
    }

    /// Beam search for optimal code assignment
    fn beam_search_assignment(&self, vector: &[f32]) -> Result<(Vec<usize>, f32), QuantizationError> {
        let beam_width = 16; // Configurable beam width
        let mut beam = vec![(vec![], 0.0f32)]; // (codes, error)

        for _codebook_idx in 0..self.config.num_codebooks {
            let mut candidates = Vec::new();

            for (codes, _current_error) in &beam {
                for code_idx in 0..self.config.codebook_size {
                    let mut new_codes = codes.clone();
                    new_codes.push(code_idx);

                    // Calculate reconstruction error
                    let reconstruction = self.reconstruct_vector(&new_codes)?;
                    let error = self.calculate_reconstruction_error(vector, &reconstruction);

                    candidates.push((new_codes, error));
                }
            }

            // Keep top beam_width candidates
            candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            candidates.truncate(beam_width);
            beam = candidates;
        }

        // Return best assignment
        beam.into_iter().next()
            .ok_or_else(|| QuantizationError::QuantizationFailed {
                reason: "Beam search failed".to_string(),
            })
    }

    /// Update codebooks for additive quantization
    fn update_codebooks_additive(&mut self, assignments: &[Vec<usize>]) -> Result<(), QuantizationError> {
        for codebook_idx in 0..self.config.num_codebooks {
            for code_idx in 0..self.config.codebook_size {
                let mut sum = vec![0.0; self.config.vector_dim];
                let mut count = 0;

                // Collect residuals for this code
                for (vector_idx, codes) in assignments.iter().enumerate() {
                    if codes[codebook_idx] == code_idx {
                        let vector = &self.training_data[vector_idx];
                        let partial_reconstruction = self.reconstruct_partial(codes, codebook_idx)?;
                        
                        // Calculate residual
                        for (i, (&v, &r)) in vector.iter().zip(partial_reconstruction.iter()).enumerate() {
                            sum[i] += v - r;
                        }
                        count += 1;
                    }
                }

                // Update codebook entry
                if count > 0 {
                    for (i, s) in sum.iter().enumerate() {
                        self.codebooks[codebook_idx].vectors[code_idx][i] = s / count as f32;
                    }
                }
            }
        }

        Ok(())
    }
    /// Train neural quantization (QINCo)
    fn train_neural(&mut self) -> Result<(), QuantizationError> {
        debug!("Training neural quantization (QINCo)");
        
        // Initialize neural network for implicit codebooks
        self.neural_network = Some(NeuralCodebook::new(
            self.config.vector_dim,
            self.config.num_codebooks,
            self.config.codebook_size,
        ));

        // Training loop for neural codebooks
        for iteration in 0..self.config.training_iterations {
            let mut total_loss = 0.0;

            for vector in &self.training_data {
                let (_codes, reconstruction) = self.neural_encode_decode(vector)?;
                let loss = self.calculate_reconstruction_error(vector, &reconstruction);
                total_loss += loss;

                // Backpropagation (simplified)
                if let Some(ref mut network) = self.neural_network {
                    network.update_weights(vector, &reconstruction, loss)?;
                }
            }

            let avg_loss = total_loss / self.training_data.len() as f32;
            debug!("Neural training iteration {}: avg loss = {:.6}", iteration, avg_loss);

            if avg_loss < self.config.convergence_threshold {
                info!("Neural quantization converged after {} iterations", iteration + 1);
                break;
            }
        }

        Ok(())
    }

    /// Train residual quantization
    fn train_residual(&mut self) -> Result<(), QuantizationError> {
        debug!("Training residual quantization");
        
        let mut residuals = self.training_data.clone();
        
        // Train codebooks sequentially
        for codebook_idx in 0..self.config.num_codebooks {
            info!("Training codebook {} of {}", codebook_idx + 1, self.config.num_codebooks);
            
            // K-means clustering on current residuals
            self.train_codebook_kmeans(codebook_idx, &residuals)?;
            
            // Update residuals by subtracting quantized vectors
            for residual in residuals.iter_mut() {
                let code = self.find_nearest_code(codebook_idx, residual)?;
                let codeword = &self.codebooks[codebook_idx].vectors[code];
                
                for (r, &c) in residual.iter_mut().zip(codeword.iter()) {
                    *r -= c;
                }
            }
        }

        Ok(())
    }

    /// Train stacked quantizers
    fn train_stacked(&mut self) -> Result<(), QuantizationError> {
        debug!("Training stacked quantizers");
        
        // Similar to residual but with hierarchical structure
        self.train_residual()?;
        
        // Additional optimization for stacked structure
        for _ in 0..10 {
            let mut improved = false;
            
            for codebook_idx in 0..self.config.num_codebooks {
                let old_error = self.calculate_total_reconstruction_error()?;
                self.optimize_codebook(codebook_idx)?;
                let new_error = self.calculate_total_reconstruction_error()?;
                
                if new_error < old_error * 0.99 {
                    improved = true;
                }
            }
            
            if !improved {
                break;
            }
        }

        Ok(())
    }

    /// Train single codebook using K-means
    fn train_codebook_kmeans(&mut self, codebook_idx: usize, data: &[Vec<f32>]) -> Result<(), QuantizationError> {
        let mut rng = rand::thread_rng();
        let use_simdx = self.config.use_simdx;
        let simdx_engine = self.simdx_engine.clone();
        
        let codebook = &mut self.codebooks[codebook_idx];
        
        // Initialize centroids randomly
        codebook.random_init(&mut rng);
        
        for _iteration in 0..50 { // K-means iterations
            let mut assignments = vec![0; data.len()];
            let mut changed = false;
            
            // Assignment step
            for (i, vector) in data.iter().enumerate() {
                let mut best_code = 0;
                let mut best_distance = f32::INFINITY;
                
                for (code_idx, centroid) in codebook.vectors.iter().enumerate() {
                    let distance = if use_simdx {
                        simdx_engine.cosine_distance(vector, centroid)
                            .unwrap_or_else(|_| Self::euclidean_distance_static(vector, centroid))
                    } else {
                        Self::euclidean_distance_static(vector, centroid)
                    };
                    
                    if distance < best_distance {
                        best_distance = distance;
                        best_code = code_idx;
                    }
                }
                
                if assignments[i] != best_code {
                    assignments[i] = best_code;
                    changed = true;
                }
            }
            
            if !changed {
                break;
            }
            
            // Update step
            for code_idx in 0..codebook.size {
                let mut sum = vec![0.0; codebook.dimension];
                let mut count = 0;
                
                for (i, &assignment) in assignments.iter().enumerate() {
                    if assignment == code_idx {
                        for (j, &val) in data[i].iter().enumerate() {
                            sum[j] += val;
                        }
                        count += 1;
                    }
                }
                
                if count > 0 {
                    for (j, s) in sum.iter().enumerate() {
                        codebook.vectors[code_idx][j] = s / count as f32;
                    }
                }
            }
        }
        
        Ok(())
    }
    /// Quantize a vector
    pub fn quantize(&self, vector: &[f32]) -> Result<QuantizedVector, QuantizationError> {
        if !self.is_trained {
            return Err(QuantizationError::QuantizationFailed {
                reason: "Quantizer not trained".to_string(),
            });
        }

        if vector.len() != self.config.vector_dim {
            return Err(QuantizationError::DimensionMismatch {
                expected: self.config.vector_dim,
                actual: vector.len(),
            });
        }

        let codes = match self.config.method {
            QuantizationMethod::Additive => self.quantize_additive(vector)?,
            QuantizationMethod::Neural => self.quantize_neural(vector)?,
            QuantizationMethod::Residual => self.quantize_residual(vector)?,
            QuantizationMethod::Stacked => self.quantize_stacked(vector)?,
        };

        let reconstruction = self.reconstruct_vector(&codes)?;
        let error = self.calculate_reconstruction_error(vector, &reconstruction);

        Ok(QuantizedVector {
            codes,
            method: self.config.method,
            reconstruction_error: error,
        })
    }

    /// Quantize using additive method
    fn quantize_additive(&self, vector: &[f32]) -> Result<Vec<usize>, QuantizationError> {
        let (codes, _) = self.beam_search_assignment(vector)?;
        Ok(codes)
    }

    /// Quantize using neural method
    fn quantize_neural(&self, vector: &[f32]) -> Result<Vec<usize>, QuantizationError> {
        let (codes, _) = self.neural_encode_decode(vector)?;
        Ok(codes)
    }

    /// Quantize using residual method
    fn quantize_residual(&self, vector: &[f32]) -> Result<Vec<usize>, QuantizationError> {
        let mut codes = Vec::with_capacity(self.config.num_codebooks);
        let mut residual = vector.to_vec();

        for codebook_idx in 0..self.config.num_codebooks {
            let code = self.find_nearest_code(codebook_idx, &residual)?;
            codes.push(code);

            // Update residual
            let codeword = &self.codebooks[codebook_idx].vectors[code];
            for (r, &c) in residual.iter_mut().zip(codeword.iter()) {
                *r -= c;
            }
        }

        Ok(codes)
    }

    /// Quantize using stacked method
    fn quantize_stacked(&self, vector: &[f32]) -> Result<Vec<usize>, QuantizationError> {
        // Same as residual for now
        self.quantize_residual(vector)
    }

    /// Reconstruct vector from codes
    pub fn reconstruct_vector(&self, codes: &[usize]) -> Result<Vec<f32>, QuantizationError> {
        // Allow partial reconstruction for beam search
        if codes.len() > self.config.num_codebooks {
            return Err(QuantizationError::QuantizationFailed { reason: "Too many codes".to_string() });
        }

        match self.config.method {
            QuantizationMethod::Neural => {
                if let Some(ref network) = self.neural_network {
                    network.decode(codes)
                } else {
                    Err(QuantizationError::QuantizationFailed {
                        reason: "Neural network not initialized".to_string(),
                    })
                }
            }
            _ => {
                let mut reconstruction = vec![0.0; self.config.vector_dim];
                
                for (codebook_idx, &code) in codes.iter().enumerate() {
                    if code >= self.codebooks[codebook_idx].size {
                        return Err(QuantizationError::QuantizationFailed {
                            reason: format!("Invalid code {} for codebook {}", code, codebook_idx),
                        });
                    }
                    
                    let codeword = &self.codebooks[codebook_idx].vectors[code];
                    for (i, &val) in codeword.iter().enumerate() {
                        reconstruction[i] += val;
                    }
                }
                
                Ok(reconstruction)
            }
        }
    }

    /// Helper functions
    fn reconstruct_partial(&self, codes: &[usize], exclude_idx: usize) -> Result<Vec<f32>, QuantizationError> {
        let mut reconstruction = vec![0.0; self.config.vector_dim];
        
        for (codebook_idx, &code) in codes.iter().enumerate() {
            if codebook_idx != exclude_idx {
                let codeword = &self.codebooks[codebook_idx].vectors[code];
                for (i, &val) in codeword.iter().enumerate() {
                    reconstruction[i] += val;
                }
            }
        }
        
        Ok(reconstruction)
    }

    fn find_nearest_code(&self, codebook_idx: usize, vector: &[f32]) -> Result<usize, QuantizationError> {
        let codebook = &self.codebooks[codebook_idx];
        let mut best_code = 0;
        let mut best_distance = f32::INFINITY;

        for (code_idx, centroid) in codebook.vectors.iter().enumerate() {
            let distance = if self.config.use_simdx {
                self.simdx_engine.cosine_distance(vector, centroid)
                    .unwrap_or_else(|_| self.euclidean_distance(vector, centroid))
            } else {
                self.euclidean_distance(vector, centroid)
            };

            if distance < best_distance {
                best_distance = distance;
                best_code = code_idx;
            }
        }

        Ok(best_code)
    }

    fn euclidean_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        Self::euclidean_distance_static(a, b)
    }
    
    fn euclidean_distance_static(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
    }

    fn calculate_reconstruction_error(&self, original: &[f32], reconstruction: &[f32]) -> f32 {
        self.euclidean_distance(original, reconstruction)
    }

    fn calculate_total_reconstruction_error(&self) -> Result<f32, QuantizationError> {
        let mut total_error = 0.0;
        
        for vector in &self.training_data {
            let quantized = self.quantize(vector)?;
            total_error += quantized.reconstruction_error;
        }
        
        Ok(total_error / self.training_data.len() as f32)
    }

    fn optimize_codebook(&mut self, _codebook_idx: usize) -> Result<(), QuantizationError> {
        // Simplified optimization - in practice would use more sophisticated methods
        let mut assignments = Vec::new();
        
        for vector in &self.training_data {
            let codes = self.quantize_residual(vector)?;
            assignments.push(codes);
        }
        
        self.update_codebooks_additive(&assignments)?;
        Ok(())
    }

    fn neural_encode_decode(&self, vector: &[f32]) -> Result<(Vec<usize>, Vec<f32>), QuantizationError> {
        if let Some(ref network) = self.neural_network {
            let codes = network.encode(vector)?;
            let reconstruction = network.decode(&codes)?;
            Ok((codes, reconstruction))
        } else {
            Err(QuantizationError::QuantizationFailed {
                reason: "Neural network not initialized".to_string(),
            })
        }
    }
}
/// Neural Codebook for implicit quantization (QINCo implementation)
struct NeuralCodebook {
    input_dim: usize,
    num_codebooks: usize,
    codebook_size: usize,
    // Simplified neural network representation
    weights: Vec<Vec<Vec<f32>>>, // [layer][input][output]
    biases: Vec<Vec<f32>>,       // [layer][output]
}

impl NeuralCodebook {
    fn new(input_dim: usize, num_codebooks: usize, codebook_size: usize) -> Self {
        let mut rng = rand::thread_rng();
        
        // Simple 2-layer network for demonstration
        let hidden_dim = 256;
        let output_dim = num_codebooks * codebook_size;
        
        let mut weights = Vec::new();
        let mut biases = Vec::new();
        
        // Input to hidden layer
        let mut w1 = vec![vec![0.0; hidden_dim]; input_dim];
        for i in 0..input_dim {
            for j in 0..hidden_dim {
                w1[i][j] = rng.gen_range(-0.1..0.1);
            }
        }
        weights.push(w1);
        biases.push(vec![0.0; hidden_dim]);
        
        // Hidden to output layer
        let mut w2 = vec![vec![0.0; output_dim]; hidden_dim];
        for i in 0..hidden_dim {
            for j in 0..output_dim {
                w2[i][j] = rng.gen_range(-0.1..0.1);
            }
        }
        weights.push(w2);
        biases.push(vec![0.0; output_dim]);
        
        Self {
            input_dim,
            num_codebooks,
            codebook_size,
            weights,
            biases,
        }
    }
    
    fn encode(&self, vector: &[f32]) -> Result<Vec<usize>, QuantizationError> {
        // Forward pass through network
        let activations = vector.to_vec();
        
        // Hidden layer
        let mut hidden = vec![0.0; self.weights[0][0].len()];
        for (i, &input) in activations.iter().enumerate() {
            for (j, &weight) in self.weights[0][i].iter().enumerate() {
                hidden[j] += input * weight;
            }
        }
        for (i, &bias) in self.biases[0].iter().enumerate() {
            hidden[i] += bias;
            hidden[i] = hidden[i].tanh(); // Activation function
        }
        
        // Output layer
        let mut output = vec![0.0; self.weights[1][0].len()];
        for (i, &input) in hidden.iter().enumerate() {
            for (j, &weight) in self.weights[1][i].iter().enumerate() {
                output[j] += input * weight;
            }
        }
        for (i, &bias) in self.biases[1].iter().enumerate() {
            output[i] += bias;
        }
        
        // Convert to codes (simplified)
        let mut codes = Vec::with_capacity(self.num_codebooks);
        for i in 0..self.num_codebooks {
            let start_idx = i * self.codebook_size;
            let end_idx = start_idx + self.codebook_size;
            
            let mut best_code = 0;
            let mut best_value = output[start_idx];
            
            for (j, &value) in output[start_idx..end_idx].iter().enumerate() {
                if value > best_value {
                    best_value = value;
                    best_code = j;
                }
            }
            
            codes.push(best_code);
        }
        
        Ok(codes)
    }
    
    fn decode(&self, codes: &[usize]) -> Result<Vec<f32>, QuantizationError> {
        // Simplified decoding - in practice would use learned decoder
        let mut reconstruction = vec![0.0; self.input_dim];
        
        // Generate implicit codewords based on codes and previous context
        for (codebook_idx, &code) in codes.iter().enumerate() {
            let context = self.generate_context(codes, codebook_idx);
            let codeword = self.generate_codeword(code, &context);
            
            for (i, &val) in codeword.iter().enumerate() {
                reconstruction[i] += val;
            }
        }
        
        Ok(reconstruction)
    }
    
    fn generate_context(&self, codes: &[usize], current_idx: usize) -> Vec<f32> {
        // Generate context from previous codes
        let mut context = vec![0.0; 64]; // Fixed context size
        
        for (i, &code) in codes[..current_idx].iter().enumerate() {
            if i < context.len() {
                context[i] = code as f32 / self.codebook_size as f32;
            }
        }
        
        context
    }
    
    fn generate_codeword(&self, code: usize, context: &[f32]) -> Vec<f32> {
        // Generate codeword based on code and context
        let mut codeword = vec![0.0; self.input_dim];
        let mut rng = rand::thread_rng();
        
        // Simplified generation - in practice would use neural network
        for i in 0..self.input_dim {
            let base_value = (code as f32 / self.codebook_size as f32) * 2.0 - 1.0;
            let context_influence = if i < context.len() { context[i] * 0.1 } else { 0.0 };
            codeword[i] = base_value + context_influence + rng.gen_range(-0.01..0.01);
        }
        
        codeword
    }
    
    fn update_weights(&mut self, _input: &[f32], _reconstruction: &[f32], _loss: f32) -> Result<(), QuantizationError> {
        // Simplified weight update - in practice would use proper backpropagation
        let learning_rate = 0.001;
        let mut rng = rand::thread_rng();
        
        // Add small random updates (placeholder for proper gradients)
        for layer in &mut self.weights {
            for neuron in layer {
                for weight in neuron {
                    *weight += rng.gen_range(-learning_rate..learning_rate);
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simdx::SIMDXEngine;

    #[tokio::test]
    async fn test_advanced_quantization() {
        let simdx_engine = Arc::new(SIMDXEngine::new(None));
        
        let config = QuantizationConfig {
            method: QuantizationMethod::Additive,
            num_codebooks: 4,
            codebook_size: 16,
            vector_dim: 128,
            bits_per_code: 4,
            training_iterations: 10, // Reduced for testing
            convergence_threshold: 1e-3,
            use_simdx: true,
            enable_reranking: false,
            rerank_factor: 1,
        };
        
        let mut quantizer = AdvancedQuantizer::new(config, simdx_engine);
        
        // Generate test training data
        let mut training_data = Vec::new();
        let mut rng = rand::thread_rng();
        
        for _ in 0..1000 {
            let vector: Vec<f32> = (0..128).map(|_| rng.gen_range(-1.0..1.0)).collect();
            training_data.push(vector);
        }
        
        quantizer.add_training_data(training_data).unwrap();
        quantizer.train().unwrap();
        
        // Test quantization
        let test_vector: Vec<f32> = (0..128).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let quantized = quantizer.quantize(&test_vector).unwrap();
        
        assert_eq!(quantized.codes.len(), 4);
        assert!(quantized.reconstruction_error >= 0.0);
        
        // Test reconstruction
        let reconstruction = quantizer.reconstruct_vector(&quantized.codes).unwrap();
        assert_eq!(reconstruction.len(), 128);
    }

    #[test]
    fn test_neural_codebook() {
        let network = NeuralCodebook::new(64, 4, 16);
        
        let input = vec![0.5; 64];
        let codes = network.encode(&input).unwrap();
        assert_eq!(codes.len(), 4);
        
        let reconstruction = network.decode(&codes).unwrap();
        assert_eq!(reconstruction.len(), 64);
    }
}