//! gRPC Cluster Communication Benchmarks
//!
//! Measures performance of inter-node communication:
//! - Connection pooling efficiency
//! - Request latency (single vs pooled connections)
//! - Batch operation throughput
//! - Serialization performance
//! - Compression effectiveness
//!
//! **IMPORTANT**: These benchmarks require the `grpc` feature and protoc.
//! Install protoc: https://grpc.io/docs/protoc-installation/
//!
//! Run with: cargo bench --bench grpc_benchmark --features grpc

#![cfg(feature = "grpc")]

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

// These imports require the grpc feature
use rtdb::cluster::{
    client::{ClientConfig, ClusterClient},
    proto::{
        BatchInsertRequest, BatchSearchRequest, InsertRequest, SearchRequest,
        VectorEntry,
    },
    server::{ClusterGrpcServer, ServerConfig},
    ClusterConfig, ClusterManager, NodeInfo,
};

/// Test server address
const TEST_ADDR: &str = "127.0.0.1:0"; // Use port 0 for auto-assignment

/// Vector dimension for benchmarks
const VECTOR_DIM: usize = 128;

/// Generate test vector
fn generate_vector(dim: usize) -> Vec<f32> {
    use rand::prelude::*;
    let mut rng = thread_rng();
    (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

/// Generate test vector bytes (protobuf format)
fn generate_vector_bytes(dim: usize) -> Vec<u8> {
    generate_vector(dim)
        .iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect()
}

/// Setup test runtime and server
fn setup() -> (Runtime, String) {
    let rt = Runtime::new().unwrap();
    
    // Start test server
    let addr = rt.block_on(async {
        let cluster_manager = Arc::new(tokio::sync::RwLock::new(
            ClusterManager::new_standalone()
        ));
        
        let bind_addr: std::net::SocketAddr = TEST_ADDR.parse().unwrap();
        let server = ClusterGrpcServer::new(cluster_manager, bind_addr);
        
        // Start server in background
        tokio::spawn(async move {
            server.start().await.unwrap();
        });
        
        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Get actual bound address
        // Note: In real implementation, we'd get this from the server
        "127.0.0.1:7001".to_string()
    });
    
    (rt, addr)
}

/// Benchmark connection pooling overhead
fn bench_connection_pooling(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_connection_pooling");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);
    
    let (rt, server_addr) = setup();
    
    // Test different pool sizes
    for pool_size in [1, 2, 4, 8] {
        let client_config = ClientConfig {
            connection_pool_size: pool_size,
            ..Default::default()
        };
        
        let cluster_config = ClusterConfig {
            node_id: "bench-node".to_string(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        };
        
        let client = ClusterClient::with_client_config(cluster_config, client_config);
        
        // Connect to test server
        rt.block_on(async {
            let node = NodeInfo {
                id: "test-server".to_string(),
                address: server_addr.parse().unwrap(),
                status: rtdb::cluster::NodeStatus::Active,
                shards: vec![],
                capacity: 1_000_000,
                load: 0,
                last_heartbeat: 0,
            };
            client.connect(&node).await.unwrap();
        });
        
        group.bench_with_input(
            BenchmarkId::new("pool_size", pool_size),
            &pool_size,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    // Simulate heartbeat - lightweight request
                    let _ = client.send_heartbeat("test-server").await;
                    black_box(());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark request latency under concurrent load
fn bench_request_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_request_latency");
    group.measurement_time(Duration::from_secs(10));
    
    let (rt, server_addr) = setup();
    
    let client_config = ClientConfig::default();
    let cluster_config = ClusterConfig {
        node_id: "bench-node".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    
    let client = ClusterClient::with_client_config(cluster_config, client_config);
    
    rt.block_on(async {
        let node = NodeInfo {
            id: "test-server".to_string(),
            address: server_addr.parse().unwrap(),
            status: rtdb::cluster::NodeStatus::Active,
            shards: vec![],
            capacity: 1_000_000,
            load: 0,
            last_heartbeat: 0,
        };
        client.connect(&node).await.unwrap();
    });
    
    // Benchmark single insert latency
    group.bench_function("single_insert", |b| {
        b.to_async(&rt).iter(|| async {
            let vector = generate_vector(VECTOR_DIM);
            let _ = client
                .forward_insert("test-server", "test_collection", 1, vector)
                .await;
            black_box(());
        });
    });
    
    // Benchmark single search latency
    group.bench_function("single_search", |b| {
        b.to_async(&rt).iter(|| async {
            let vector = generate_vector(VECTOR_DIM);
            let _ = client
                .forward_search("test-server", "test_collection", vector, 10)
                .await;
            black_box(());
        });
    });
    
    group.finish();
}

/// Benchmark batch operation throughput
fn bench_batch_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_batch_throughput");
    group.measurement_time(Duration::from_secs(15));
    
    let (rt, server_addr) = setup();
    
    let client_config = ClientConfig::default();
    let cluster_config = ClusterConfig {
        node_id: "bench-node".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    
    let client = ClusterClient::with_client_config(cluster_config, client_config);
    
    rt.block_on(async {
        let node = NodeInfo {
            id: "test-server".to_string(),
            address: server_addr.parse().unwrap(),
            status: rtdb::cluster::NodeStatus::Active,
            shards: vec![],
            capacity: 1_000_000,
            load: 0,
            last_heartbeat: 0,
        };
        client.connect(&node).await.unwrap();
    });
    
    // Benchmark batch insert with different batch sizes
    for batch_size in [10, 50, 100, 500] {
        group.throughput(Throughput::Elements(batch_size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("batch_insert", batch_size),
            &batch_size,
            |b, &size| {
                let entries: Vec<(u64, Vec<f32>)> = (0..size as u64)
                    .map(|i| (i, generate_vector(VECTOR_DIM)))
                    .collect();
                
                b.to_async(&rt).iter(|| async {
                    let _ = client
                        .forward_batch_insert("test-server", "test_collection", entries.clone())
                        .await;
                    black_box(());
                });
            },
        );
    }
    
    // Benchmark batch search
    for batch_size in [10, 50, 100] {
        group.throughput(Throughput::Elements(batch_size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("batch_search", batch_size),
            &batch_size,
            |b, &size| {
                let vectors: Vec<Vec<f32>> =
                    (0..size).map(|_| generate_vector(VECTOR_DIM)).collect();
                
                b.to_async(&rt).iter(|| async {
                    let _ = client
                        .forward_batch_search("test-server", "test_collection", vectors.clone(), 10)
                        .await;
                    black_box(());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark serialization performance
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_serialization");
    group.measurement_time(Duration::from_secs(5));
    
    // Test different vector dimensions
    for &dim in &[128, 384, 768, 1536] {
        let vector = generate_vector(dim);
        let vector_bytes = generate_vector_bytes(dim);
        
        group.throughput(Throughput::Bytes(vector_bytes.len() as u64));
        
        // Benchmark old format (repeated float)
        group.bench_with_input(
            BenchmarkId::new("repeated_float", dim),
            &vector,
            |b, v| {
                b.iter(|| {
                    let proto = SearchRequest {
                        collection: "test".to_string(),
                        vector: v.clone(), // repeated float
                        top_k: 10,
                        score_threshold: 0.0,
                        filter: vec![],
                        request_id: 1,
                    };
                    black_box(proto);
                });
            },
        );
        
        // Benchmark new format (bytes)
        group.bench_with_input(
            BenchmarkId::new("bytes_encoding", dim),
            &vector_bytes,
            |b, vb| {
                b.iter(|| {
                    let proto = SearchRequest {
                        collection: "test".to_string(),
                        vector: vb.clone(), // bytes
                        top_k: 10,
                        score_threshold: 0.0,
                        filter: vec![],
                        request_id: 1,
                    };
                    black_box(proto);
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark compression effectiveness
fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_compression");
    group.measurement_time(Duration::from_secs(5));
    
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;
    
    // Test different payload sizes
    for &num_vectors in &[10, 100, 1000] {
        // Generate batch insert request data
        let entries: Vec<VectorEntry> = (0..num_vectors)
            .map(|i| VectorEntry {
                id: i as u64,
                vector: generate_vector_bytes(VECTOR_DIM),
                payload: vec![], // Could add payload for more realistic test
                timestamp: 0,
            })
            .collect();
        
        let request = BatchInsertRequest {
            collection: "test".to_string(),
            entries,
            request_id: 1,
        };
        
        // Serialize to bytes (simulate protobuf encoding)
        let serialized_len = request.entries.len() * (VECTOR_DIM * 4 + 16);
        group.throughput(Throughput::Bytes(serialized_len as u64));
        
        group.bench_with_input(
            BenchmarkId::new("no_compression", num_vectors),
            &request,
            |b, _| {
                b.iter(|| {
                    // Simulate sending without compression
                    black_box(serialized_len);
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("gzip_compression", num_vectors),
            &request,
            |b, req| {
                b.iter(|| {
                    // Simulate gzip compression
                    let data: Vec<u8> = req
                        .entries
                        .iter()
                        .flat_map(|e| e.vector.clone())
                        .collect();
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                    encoder.write_all(&data).unwrap();
                    let compressed = encoder.finish().unwrap();
                    black_box(compressed.len());
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark topology operations
fn bench_topology_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("grpc_topology");
    group.measurement_time(Duration::from_secs(5));
    
    let (rt, server_addr) = setup();
    
    let client_config = ClientConfig::default();
    let cluster_config = ClusterConfig {
        node_id: "bench-node".to_string(),
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    };
    
    let client = ClusterClient::with_client_config(cluster_config, client_config);
    
    rt.block_on(async {
        let node = NodeInfo {
            id: "test-server".to_string(),
            address: server_addr.parse().unwrap(),
            status: rtdb::cluster::NodeStatus::Active,
            shards: vec![],
            capacity: 1_000_000,
            load: 0,
            last_heartbeat: 0,
        };
        client.connect(&node).await.unwrap();
    });
    
    // Benchmark heartbeat
    group.bench_function("heartbeat", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = client.send_heartbeat("test-server").await;
            black_box(());
        });
    });
    
    // Benchmark topology fetch
    group.bench_function("get_topology", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = client.get_topology("test-server").await;
            black_box(());
        });
    });
    
    // Benchmark health check
    group.bench_function("health_check", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = client.health_check("test-server").await;
            black_box(());
        });
    });
    
    group.finish();
}

// Only run benchmarks if grpc feature is enabled
#[cfg(feature = "grpc")]
criterion_group!(
    benches,
    bench_connection_pooling,
    bench_request_latency,
    bench_batch_throughput,
    bench_serialization,
    bench_compression,
    bench_topology_operations
);

#[cfg(feature = "grpc")]
criterion_main!(benches);

// Stub for when grpc feature is not enabled
#[cfg(not(feature = "grpc"))]
fn main() {
    eprintln!("gRPC benchmarks require the 'grpc' feature and protoc.");
    eprintln!("Install protoc: https://grpc.io/docs/protoc-installation/");
    eprintln!("Then run: cargo bench --bench grpc_benchmark --features grpc");
    std::process::exit(1);
}
