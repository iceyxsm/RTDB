// Integration tests for new RTDB production features

use rtdb::{
    simdx::SIMDXEngine,
    quantization::advanced::{AdvancedQuantizer, QuantizationConfig as AdvancedQuantizationConfig, QuantizationMethod},
    Distance, Vector,
};
use std::sync::Arc;

#[tokio::test]
async fn test_simdx_engine() {
    let engine = SIMDXEngine::new(None);
    
    // Test basic functionality
    let capabilities = engine.get_capabilities();
    println!("SIMDX Capabilities: {:?}", capabilities);
    
    // Test cosine distance
    let a = vec![1.0, 2.0, 3.0, 4.0];
    let b = vec![1.0, 2.0, 3.0, 4.0];
    
    let distance = engine.cosine_distance(&a, &b).expect("Cosine distance failed");
    println!("Cosine distance (identical vectors): {}", distance);
    assert!((distance - 0.0).abs() < 1e-6);
    
    // Test different vectors
    let c = vec![4.0, 3.0, 2.0, 1.0];
    let distance2 = engine.cosine_distance(&a, &c).expect("Cosine distance failed");
    println!("Cosine distance (different vectors): {}", distance2);
    assert!(distance2 > 0.0);
    
    // Test batch operations
    let vectors = vec![
        vec![1.0, 2.0, 3.0, 4.0],
        vec![2.0, 3.0, 4.0, 5.0],
        vec![3.0, 4.0, 5.0, 6.0],
    ];
    
    let batch_distances = engine.batch_cosine_distance(&a, &vectors).expect("Batch distance failed");
    println!("Batch distances: {:?}", batch_distances);
    assert_eq!(batch_distances.len(), 3);
    
    // Get metrics
    let metrics = engine.get_metrics();
    println!("SIMDX Metrics: {:?}", metrics);
    assert!(metrics.operations_count > 0);
}

#[tokio::test]
async fn test_advanced_quantization() {
    let simdx_engine = Arc::new(SIMDXEngine::new(None));
    
    let config = AdvancedQuantizationConfig {
        method: QuantizationMethod::Additive,
        num_codebooks: 8,
        codebook_size: 256,
        vector_dim: 128,
        bits_per_code: 8,
        training_iterations: 10, // Reduced for testing
        convergence_threshold: 1e-3,
        use_simdx: true,
        enable_reranking: false,
        rerank_factor: 1,
    };
    
    let mut quantizer = AdvancedQuantizer::new(config, simdx_engine);
    
    // Generate test training data
    let training_vectors: Vec<Vec<f32>> = (0..2048)
        .map(|i| {
            (0..128)
                .map(|j| ((i * 128 + j) as f32).sin())
                .collect()
        })
        .collect();
    
    // Add training data
    quantizer.add_training_data(training_vectors.clone()).expect("Failed to add training data");
    
    // Train quantizer
    quantizer.train().expect("Training failed");
    println!("Quantization training completed");
    
    // Test quantization
    let test_vector = &training_vectors[0];
    let quantized = quantizer.quantize(test_vector).expect("Quantization failed");
    
    println!("Original vector length: {}", test_vector.len());
    println!("Quantized codes length: {}", quantized.codes.len());
    println!("Reconstruction error: {:.6}", quantized.reconstruction_error);
    
    // Test reconstruction
    let reconstructed = quantizer.reconstruct_vector(&quantized.codes).expect("Reconstruction failed");
    println!("Reconstructed vector length: {}", reconstructed.len());
    
    // Verify reconstruction quality
    let original_norm: f32 = test_vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    let reconstructed_norm: f32 = reconstructed.iter().map(|x| x * x).sum::<f32>().sqrt();
    println!("Original norm: {:.6}, Reconstructed norm: {:.6}", original_norm, reconstructed_norm);
}

#[test]
fn test_distance_calculations() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    
    // Test Euclidean distance
    let euclidean = Distance::Euclidean.calculate(&a, &b).expect("Euclidean failed");
    println!("Euclidean distance: {}", euclidean);
    assert!((euclidean - 1.414213562).abs() < 1e-6);
    
    // Test Cosine distance
    let cosine = Distance::Cosine.calculate(&a, &b).expect("Cosine failed");
    println!("Cosine distance: {}", cosine);
    assert!((cosine - 1.0).abs() < 1e-6); // Orthogonal vectors
    
    // Test Dot product
    let dot = Distance::Dot.calculate(&a, &b).expect("Dot product failed");
    println!("Dot product: {}", dot);
    assert!((dot - 0.0).abs() < 1e-6);
    
    // Test Manhattan distance
    let manhattan = Distance::Manhattan.calculate(&a, &b).expect("Manhattan failed");
    println!("Manhattan distance: {}", manhattan);
    assert!((manhattan - 2.0).abs() < 1e-6);
}

#[test]
fn test_vector_operations() {
    let mut vector = Vector::new(vec![3.0, 4.0, 0.0]);
    
    // Test L2 norm
    let norm = vector.l2_norm();
    println!("L2 norm: {}", norm);
    assert!((norm - 5.0).abs() < 1e-6);
    
    // Test normalization
    vector.normalize();
    let new_norm = vector.l2_norm();
    println!("Normalized L2 norm: {}", new_norm);
    assert!((new_norm - 1.0).abs() < 1e-6);
    
    // Test dimension
    assert_eq!(vector.dim(), 3);
}

#[test]
fn test_vector_with_payload() {
    use serde_json::json;
    
    let payload = json!({
        "category": "test",
        "score": 0.95,
        "metadata": {
            "source": "unit_test"
        }
    }).as_object().unwrap().clone();
    
    let vector = Vector::with_payload(vec![1.0, 2.0, 3.0], payload);
    
    assert_eq!(vector.dim(), 3);
    assert!(vector.payload.is_some());
    
    if let Some(ref payload) = vector.payload {
        assert_eq!(payload.get("category").unwrap().as_str().unwrap(), "test");
        assert_eq!(payload.get("score").unwrap().as_f64().unwrap(), 0.95);
    }
}