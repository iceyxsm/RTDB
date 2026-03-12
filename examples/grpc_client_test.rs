//! Simple gRPC client test for RTDB
//! 
//! This example demonstrates how to use the gRPC API to:
//! - Create a collection
//! - Upsert vectors
//! - Search for similar vectors
//! - Delete vectors
//! 
//! Run the server first: cargo run --features grpc -- start
//! Then run this example: cargo run --example grpc_client_test --features grpc

use tonic::transport::Channel;

// Import generated proto types
mod proto {
    include!("../src/api/generated/rtdb.rs");
}

use proto::{
    collections_client::CollectionsClient,
    points_client::PointsClient,
    CreateCollectionRequest, VectorParams, Distance,
    UpsertPointsRequest, PointStruct,
    SearchPointsRequest,
    DeletePointsRequest,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("RTDB gRPC Client Test");
    println!("====================\n");

    // Connect to the gRPC server
    let channel = Channel::from_static("http://127.0.0.1:6334")
        .connect()
        .await?;

    let mut collections_client = CollectionsClient::new(channel.clone());
    let mut points_client = PointsClient::new(channel);

    // 1. Create a collection
    println!("1. Creating collection 'test_collection'...");
    let create_request = tonic::Request::new(CreateCollectionRequest {
        collection_name: "test_collection".to_string(),
        vectors_config: Some(VectorParams {
            size: 4,
            distance: Distance::Cosine as i32,
        }),
    });

    let response = collections_client.create(create_request).await?;
    println!("   Response: {:?}\n", response.into_inner());

    // 2. Upsert some vectors
    println!("2. Upserting 3 vectors...");
    let upsert_request = tonic::Request::new(UpsertPointsRequest {
        collection_name: "test_collection".to_string(),
        points: vec![
            PointStruct {
                id: 1,
                vector: vec![1.0, 0.0, 0.0, 0.0],
            },
            PointStruct {
                id: 2,
                vector: vec![0.0, 1.0, 0.0, 0.0],
            },
            PointStruct {
                id: 3,
                vector: vec![0.0, 0.0, 1.0, 0.0],
            },
        ],
    });

    let response = points_client.upsert(upsert_request).await?;
    println!("   Response: {:?}\n", response.into_inner());

    // 3. Search for similar vectors
    println!("3. Searching for vectors similar to [1.0, 0.1, 0.0, 0.0]...");
    let search_request = tonic::Request::new(SearchPointsRequest {
        collection_name: "test_collection".to_string(),
        vector: vec![1.0, 0.1, 0.0, 0.0],
        limit: 2,
        with_payload: false,
        with_vectors: true,
    });

    let response = points_client.search(search_request).await?;
    let search_results = response.into_inner();
    println!("   Found {} results:", search_results.result.len());
    for (i, point) in search_results.result.iter().enumerate() {
        println!("     {}. ID: {}, Score: {:.4}", i + 1, point.id, point.score);
    }
    println!();

    // 4. Delete a vector
    println!("4. Deleting vector with ID 2...");
    let delete_request = tonic::Request::new(DeletePointsRequest {
        collection_name: "test_collection".to_string(),
        ids: vec![2],
    });

    let response = points_client.delete(delete_request).await?;
    println!("   Response: {:?}\n", response.into_inner());

    // 5. List collections
    println!("5. Listing all collections...");
    let list_request = tonic::Request::new(proto::ListCollectionsRequest {});
    let response = collections_client.list(list_request).await?;
    let collections = response.into_inner();
    println!("   Found {} collections:", collections.collections.len());
    for coll in collections.collections {
        println!("     - {}", coll.name);
    }

    println!("\nAll tests completed successfully!");
    Ok(())
}
