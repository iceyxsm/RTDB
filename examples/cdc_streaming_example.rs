//! CDC Streaming Example - Real-time Vector Updates
//!
//! This example demonstrates:
//! 1. Setting up CDC on a collection
//! 2. Subscribing to changes via WebSocket
//! 3. Processing events in real-time
//! 4. Using transactions for batch changes

use rtdb::collection::CollectionManager;
use rtdb::streaming::server::CdcStreamingServer;
use rtdb::streaming::{CdcConfig, CdcEngine, ChangeType};
use rtdb::{CollectionConfig, UpsertRequest, Vector};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("RTDB CDC Streaming Example");
    println!("===========================\n");

    // 1. Initialize collection manager
    let collection_manager = Arc::new(CollectionManager::new("./data_cdc_example")?);

    // Create a collection
    let config = CollectionConfig {
        dimension: 128,
        distance: rtdb::Distance::Cosine,
        hnsw_config: None,
        quantization_config: None,
        optimizer_config: None,
    };
    collection_manager.create_collection("products", config)?;

    println!("Created collection: products");

    // 2. Initialize CDC engine
    let cdc_config = CdcConfig {
        buffer_size: 10000,
        persistent: true,
        retention_secs: 3600,
        max_events_per_sec: 100000,
    };
    let cdc_engine = Arc::new(CdcEngine::new(cdc_config));

    // 3. Subscribe to changes
    let mut subscription = cdc_engine.subscribe("products").await?;
    println!("Subscribed to CDC events for 'products' collection");

    // 4. Spawn event processor
    let processor_handle = tokio::spawn(async move {
        println!("\nEvent Processor Started");
        println!("------------------------");

        loop {
            match subscription.recv().await {
                Ok(event) => {
                    println!(
                        "[{}] {} - Vector ID: {}, Type: {:?}, Seq: {}",
                        event.timestamp_us,
                        event.event_id[..8].to_string(),
                        event.vector_id,
                        event.change_type,
                        event.sequence_number
                    );

                    if let Some(ref txn_id) = event.transaction_id {
                        println!("  Transaction: {}", txn_id);
                    }
                }
                Err(e) => {
                    eprintln!("Error receiving event: {}", e);
                    break;
                }
            }
        }
    });

    // 5. Simulate vector operations
    let collection = collection_manager.get_collection("products")?;

    println!("\nSimulating vector operations...\n");

    // Insert single vector
    println!("1. Inserting single vector...");
    let vector1 = Vector::new(vec![0.1; 128]);
    collection.upsert(UpsertRequest {
        vectors: vec![(1, vector1.clone())],
    })?;

    // Emit CDC event manually (in real usage, this would be automatic)
    cdc_engine
        .emit_insert("products", 1, vector1, None)
        .await?;
    sleep(Duration::from_millis(100)).await;

    // Insert batch
    println!("2. Inserting batch of 5 vectors...");
    let mut batch = Vec::new();
    for i in 2..=6 {
        let vector = Vector::new(vec![i as f32 / 10.0; 128]);
        batch.push((i as u64, vector));
    }
    collection.upsert(UpsertRequest { vectors: batch.clone() })?;

    // Emit batch CDC events
    let changes: Vec<_> = batch
        .into_iter()
        .map(|(id, vec)| (ChangeType::Insert, id, Some(vec)))
        .collect();
    cdc_engine
        .emit_batch("products", changes, Some("txn-batch-001".to_string()))
        .await?;
    sleep(Duration::from_millis(100)).await;

    // Update vector
    println!("3. Updating vector 1...");
    let updated_vector = Vector::new(vec![0.5; 128]);
    collection.upsert(UpsertRequest {
        vectors: vec![(1, updated_vector.clone())],
    })?;
    cdc_engine
        .emit_update("products", 1, updated_vector, Some(vector1), None)
        .await?;
    sleep(Duration::from_millis(100)).await;

    // Delete vector
    println!("4. Deleting vector 3...");
    // In real usage: collection.delete(3)?;
    cdc_engine
        .emit_delete("products", 3, Some(Vector::new(vec![0.2; 128])), None)
        .await?;
    sleep(Duration::from_millis(100)).await;

    // Wait a bit for events to be processed
    sleep(Duration::from_secs(2)).await;

    // Shutdown
    println!("\nShutting down...");
    processor_handle.abort();

    println!("\nCDC Example Complete!");
    println!("\nKey Features Demonstrated:");
    println!("- Real-time event streaming");
    println!("- Transaction grouping");
    println!("- Insert/Update/Delete events");
    println!("- Event sequencing");

    Ok(())
}

/// Example: WebSocket streaming server
async fn websocket_server_example() -> Result<(), Box<dyn std::error::Error>> {
    use axum::Server;

    let cdc_engine = Arc::new(CdcEngine::new(CdcConfig::default()));
    let streaming_server = CdcStreamingServer::new(cdc_engine);

    let app = streaming_server.router();

    println!("CDC Streaming Server starting on http://localhost:8080");
    println!("WebSocket endpoint: ws://localhost:8080/ws/cdc/{collection}");
    println!("SSE endpoint: http://localhost:8080/events/cdc/{collection}");

    Server::bind(&"0.0.0.0:8080".parse()?)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

/// Example: WebSocket client
async fn websocket_client_example() -> Result<(), Box<dyn std::error::Error>> {
    use rtdb::streaming::server::CdcStreamClient;

    let client = CdcStreamClient::new("http://localhost:8080");
    let mut stream = client.subscribe_websocket("products").await?;

    println!("Connected to CDC WebSocket stream");

    while let Ok(event) = stream.recv().await {
        println!(
            "Received: {:?} for vector {} in collection {}",
            event.change_type, event.vector_id, event.collection
        );

        // Send ACK
        stream.ack(&event.event_id).await?;
    }

    Ok(())
}
