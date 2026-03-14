use anyhow::Result;
use rtdb::{
    client::{RtdbClient, Config},
    quantization::advanced::{QuantizationConfig, QuantizationMethod},
    cross_region::CrossRegionReplicator,
    wasm::WasmRuntime,
    multimodal::MultiModalSearchEngine,
};
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 RTDB Advanced Features Demo");
    
    // Initialize configuration
    let config = Config::default()
        .with_host("localhost")
        .with_port(8080)
        .with_quantization_enabled(true)
        .with_cross_region_enabled(true)
        .with_wasm_enabled(true);
    
    // Initialize client
    let client = RtdbClient::new(config).await?;
    
    // Demo 1: Advanced Quantization
    demo_advanced_quantization(&client).await?;
    
    // Demo 2: Cross-Region Replication
    demo_cross_region_replication(&client).await?;
    
    // Demo 3: WebAssembly Runtime
    demo_wasm_runtime(&client).await?;
    
    // Demo 4: Multi-Modal Search
    demo_multimodal_search(&client).await?;
    
    println!("✅ All advanced features demonstrated successfully!");
    Ok(())
}

async fn demo_advanced_quantization(client: &RtdbClient) -> Result<()> {
    println!("\n📊 Advanced Quantization Demo");
    
    // Configure different quantization strategies
    let quantization_configs = vec![
        QuantizationConfig {
            method: QuantizationMethod::Additive,
            num_codebooks: 8,
            codebook_size: 256,
            vector_dim: 128,
            bits_per_code: 8,
            training_iterations: 100,
            convergence_threshold: 0.001,
            use_simdx: true,
            enable_reranking: true,
            rerank_factor: 2,
        },
        QuantizationConfig {
            method: QuantizationMethod::Neural,
            num_codebooks: 4,
            codebook_size: 256,
            vector_dim: 128,
            bits_per_code: 4,
            training_iterations: 100,
            convergence_threshold: 0.001,
            use_simdx: true,
            enable_reranking: false,
            rerank_factor: 1,
        },
        QuantizationConfig {
            method: QuantizationMethod::Residual,
            num_codebooks: 1,
            codebook_size: 256,
            vector_dim: 128,
            bits_per_code: 1,
            training_iterations: 50,
            convergence_threshold: 0.01,
            use_simdx: true,
            enable_reranking: false,
            rerank_factor: 1,
        },
    ];
    
    for config in quantization_configs {
        println!("  Testing {:?} quantization...", config.method);
        
        // Create collection with quantization
        let collection_name = format!("quantized_{:?}", config.method).to_lowercase();
        client.create_collection(&collection_name, 128, Some(config)).await?;
        
        // Insert test vectors
        let vectors: Vec<Vec<f32>> = (0..1000)
            .map(|i| (0..128).map(|j| (i * j) as f32 / 1000.0).collect())
            .collect();
        
        client.insert_batch(&collection_name, vectors).await?;
        
        // Perform search
        let query_vector: Vec<f32> = (0..128).map(|i| i as f32 / 128.0).collect();
        let results = client.search(&collection_name, query_vector, 10).await?;
        
        println!("    Found {} results with quantization", results.len());
    }
    
    Ok(())
}

async fn demo_cross_region_replication(client: &RtdbClient) -> Result<()> {
    println!("\n🌍 Cross-Region Replication Demo");
    
    // Initialize cross-region replicator
    let replicator = CrossRegionReplicator::new(vec![
        "us-east-1".to_string(),
        "eu-west-1".to_string(),
        "ap-southeast-1".to_string(),
    ]).await?;
    
    // Create replicated collection
    let collection_name = "global_collection";
    client.create_collection(collection_name, 256, None).await?;
    
    // Enable replication for this collection
    replicator.enable_replication(collection_name).await?;
    
    // Insert data that will be replicated
    let vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..256).map(|j| ((i + j) as f32).sin()).collect())
        .collect();
    
    client.insert_batch(collection_name, vectors).await?;
    
    // Check replication status
    let status = replicator.get_replication_status(collection_name).await?;
    println!("  Replication status: {:?}", status);
    
    // Simulate cross-region search
    for region in &["us-east-1", "eu-west-1", "ap-southeast-1"] {
        let query_vector: Vec<f32> = (0..256).map(|i| (i as f32).cos()).collect();
        let results = replicator.search_in_region(region, collection_name, query_vector, 5).await?;
        println!("  Region {}: Found {} results", region, results.len());
    }
    
    Ok(())
}

async fn demo_wasm_runtime(client: &RtdbClient) -> Result<()> {
    println!("\n🔧 WebAssembly Runtime Demo");
    
    // Initialize WASM runtime
    let wasm_runtime = WasmRuntime::new().await?;
    
    // Load custom similarity function
    let wasm_code = r#"
        (module
            (func $custom_similarity (param $a f32) (param $b f32) (result f32)
                local.get $a
                local.get $b
                f32.sub
                f32.abs
                f32.const 1.0
                f32.sub
            )
            (export "custom_similarity" (func $custom_similarity))
        )
    "#;
    
    wasm_runtime.load_module("custom_similarity", wasm_code.as_bytes()).await?;
    
    // Create collection with custom WASM function
    let collection_name = "wasm_collection";
    client.create_collection(collection_name, 64, None).await?;
    
    // Register custom similarity function
    client.register_wasm_function(collection_name, "custom_similarity").await?;
    
    // Insert test data
    let vectors: Vec<Vec<f32>> = (0..50)
        .map(|i| (0..64).map(|j| (i as f32 + j as f32) / 100.0).collect())
        .collect();
    
    client.insert_batch(collection_name, vectors).await?;
    
    // Search using custom WASM similarity
    let query_vector: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
    let results = client.search_with_custom_similarity(
        collection_name, 
        query_vector, 
        10, 
        "custom_similarity"
    ).await?;
    
    println!("  WASM search found {} results", results.len());
    
    Ok(())
}

async fn demo_multimodal_search(client: &RtdbClient) -> Result<()> {
    println!("\n🎭 Multi-Modal Search Demo");
    
    // Initialize multi-modal search engine
    let multimodal_engine = MultiModalSearchEngine::new().await?;
    
    // Create multi-modal collection
    let collection_name = "multimodal_collection";
    client.create_multimodal_collection(collection_name).await?;
    
    // Insert different types of data
    
    // Text embeddings
    let text_data = vec![
        "The quick brown fox jumps over the lazy dog",
        "Machine learning is transforming the world",
        "Vector databases enable semantic search",
    ];
    
    for (i, text) in text_data.iter().enumerate() {
        let embedding = multimodal_engine.encode_text(text).await?;
        let metadata = json!({
            "type": "text",
            "content": text,
            "id": i
        });
        client.insert_with_metadata(collection_name, embedding, metadata).await?;
    }
    
    // Image embeddings (simulated)
    let image_paths = vec![
        "/path/to/image1.jpg",
        "/path/to/image2.jpg", 
        "/path/to/image3.jpg",
    ];
    
    for (i, path) in image_paths.iter().enumerate() {
        // In a real implementation, you would load and encode the actual image
        let embedding = multimodal_engine.encode_image_path(path).await?;
        let metadata = json!({
            "type": "image",
            "path": path,
            "id": i + 100
        });
        client.insert_with_metadata(collection_name, embedding, metadata).await?;
    }
    
    // Audio embeddings (simulated)
    let audio_paths = vec![
        "/path/to/audio1.wav",
        "/path/to/audio2.wav",
    ];
    
    for (i, path) in audio_paths.iter().enumerate() {
        let embedding = multimodal_engine.encode_audio_path(path).await?;
        let metadata = json!({
            "type": "audio", 
            "path": path,
            "id": i + 200
        });
        client.insert_with_metadata(collection_name, embedding, metadata).await?;
    }
    
    // Perform cross-modal searches
    
    // Text-to-everything search
    let text_query = "machine learning algorithms";
    let text_embedding = multimodal_engine.encode_text(&text_query).await?;
    let results = client.search_with_metadata(collection_name, text_embedding.clone(), 10).await?;
    
    println!("  Text query '{}' found {} cross-modal results:", text_query, results.len());
    for result in &results {
        if let Some(metadata) = &result.metadata {
            println!("    - Type: {}, Score: {:.3}", 
                metadata.get("type").unwrap_or(&json!("unknown")), 
                result.score
            );
        }
    }
    
    // Image-to-everything search (simulated)
    let image_query = "/path/to/query_image.jpg";
    let image_embedding = multimodal_engine.encode_image_path(&image_query).await?;
    let results = client.search_with_metadata(collection_name, image_embedding.clone(), 5).await?;
    
    println!("  Image query found {} cross-modal results", results.len());
    
    // Hybrid search combining multiple modalities
    let hybrid_results = multimodal_engine.hybrid_search(
        collection_name,
        vec![
            ("text", text_embedding),
            ("image", image_embedding),
        ],
        vec![0.7, 0.3], // weights
        10
    ).await?;
    
    println!("  Hybrid search found {} results", hybrid_results.len());
    
    Ok(())
}