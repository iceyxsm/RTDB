use reqwest;
use serde_json::json;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let base_url = "http://localhost:8333";
    
    println!("Testing RTDB API performance...");
    
    // Test 1: Create collection
    println!("1. Creating collection...");
    let start = Instant::now();
    let response = client
        .put(format!("{}/collections/test_perf", base_url))
        .json(&json!({
            "dimension": 128,
            "distance": "cosine"
        }))
        .send()
        .await?;
    println!("   Collection creation: {:?}, Status: {}", start.elapsed(), response.status());
    
    // Test 2: Insert a point
    println!("2. Inserting point...");
    let start = Instant::now();
    let response = client
        .put(format!("{}/collections/test_perf/points", base_url))
        .json(&json!({
            "points": [{
                "id": 12345,
                "vector": vec![0.1; 128]
            }]
        }))
        .send()
        .await?;
    println!("   Insert: {:?}, Status: {}", start.elapsed(), response.status());
    
    // Test 3: Get point by ID
    println!("3. Getting point by ID...");
    let start = Instant::now();
    let response = client
        .get(format!("{}/collections/test_perf/points/12345", base_url))
        .send()
        .await?;
    println!("   Get by ID: {:?}, Status: {}", start.elapsed(), response.status());
    let text = response.text().await?;
    println!("   Response: {}", text);
    
    // Test 4: Delete point
    println!("4. Deleting point...");
    let start = Instant::now();
    let response = client
        .delete(format!("{}/collections/test_perf/points/12345", base_url))
        .send()
        .await?;
    println!("   Delete: {:?}, Status: {}", start.elapsed(), response.status());
    
    // Test 5: Batch operations
    println!("5. Testing batch operations (10 inserts)...");
    let start = Instant::now();
    for i in 0..10 {
        let response = client
            .put(format!("{}/collections/test_perf/points", base_url))
            .json(&json!({
                "points": [{
                    "id": i,
                    "vector": vec![0.1; 128]
                }]
            }))
            .send()
            .await?;
        if !response.status().is_success() {
            println!("   Insert {} failed: {}", i, response.status());
        }
    }
    let batch_time = start.elapsed();
    println!("   Batch 10 inserts: {:?} ({:.2} ops/sec)", batch_time, 10.0 / batch_time.as_secs_f64());
    
    Ok(())
}