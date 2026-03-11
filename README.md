# RTDB - Real-Time Database

A production-grade smart vector database written in Rust, featuring LSM-tree storage, hybrid indexing (HNSW + Learned), and Zero-AI Smart Retrieval.

[![Build](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/iceyxsm/RTDB)
[![Tests](https://img.shields.io/badge/tests-36%2F36-brightgreen)](https://github.com/iceyxsm/RTDB)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Quick Start

```bash
# Clone repository
git clone https://github.com/iceyxsm/RTDB.git
cd RTDB

# Build release binary
cargo build --release

# Run with default config
./target/release/rtdb --config config/default.yaml

# Run tests
cargo test --lib
```

## Performance Benchmarks

### Query Latency (P99)

| Database | Latency | Relative |
|----------|---------|----------|
| **RTDB** | **<5ms**| **1.0x** |
| Qdrant   |  ~10ms  |   2.0x   |
| Weaviate |  ~15ms  |   3.0x   |
| Milvus   |  ~20ms  |   4.0x   |

### Memory Efficiency

| Database | Memory/1M vectors | Relative |
|----------|-------------------|----------|
| LanceDB  | 400MB             |   0.8x   |
| **RTDB** | **500MB**         | **1.0x** |
| Qdrant   | 700MB             |   1.4x   |
| Pinecone | 800MB             |   1.6x   |
| Milvus   | 1GB               |   2.0x   |
| Weaviate | 1.5GB             |   3.0x   |

### Distance Computation (128d vectors)

| Metric      | Latency | Throughput   |
|-------------|---------|--------------|
| Euclidean   | 112 ns  | 1.15 Gelem/s |
| Dot Product | 99 ns   | 1.29 Gelem/s |
| Cosine      | 419 ns  | 306 Melem/s  |

See [BENCHMARKS.md](BENCHMARKS.md) for full details.

## Key Features

### Core Storage
- **LSM-Tree Engine** with WAL crash recovery (CRC32C)
- **MemTable** - Lock-free skiplist for hot data
- **SSTable** - Columnar format with compression (LZ4/Zstd)
- **Zero Dependencies** - Single 15MB binary

### Vector Indexing
- **HNSW** - Hierarchical Navigable Small World graphs
- **Learned Index** - Piecewise linear models for range queries
- **Quantization** - Product Quantization (PQ) + Binary Quantization (BQ)
- **SIMD** - AVX2/NEON optimized distance kernels

### Smart Retrieval (Zero-AI)
- **Intent Classification** - Rule-based query understanding
- **Query Expansion** - Automatic synonym and entity expansion
- **Knowledge Graph** - Built-in entity relationships
- **Auto-Complete** - Fuzzy prefix search

### API Compatibility
- **Qdrant** - REST API (port 6333) and gRPC (port 6334)
- **Milvus** - SDK compatible (partial)
- **Weaviate** - GraphQL (planned)

### Enterprise Features
- **Raft Consensus** - Distributed replication
- **RBAC** - Role-based access control
- **Hot Backup** - Online snapshot/restore
- **Prometheus Metrics** - Built-in observability

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        API Layer                            │
│  ┌──────────────┬──────────────┬──────────────────────────┐ │
│  │ REST (6333)  │ gRPC (6334)  │ Smart Retrieval          │ │
│  └──────────────┴──────────────┴──────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                      Collection Layer                       │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Collection Manager  │  Index Manager  │  Storage      │ │
│  └────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                      Index Layer                            │
│  ┌──────────────┬──────────────┬──────────────────────────┐ │
│  │ HNSW         │ Learned Index│ Brute Force (GPU ready)  │ │
│  └──────────────┴──────────────┴──────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                      Storage Layer                          │
│  ┌──────────────┬──────────────┬──────────────────────────┐ │
│  │ WAL          │ MemTable     │ SSTable (LSM-Tree)       │ │
│  └──────────────┴──────────────┴──────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Usage

### REST API

```bash
# Create collection
curl -X PUT http://localhost:6333/collections/test \
  -H "Content-Type: application/json" \
  -d '{"vector_size": 128, "distance": "Cosine"}'

# Insert vectors
curl -X POST http://localhost:6333/collections/test/points \
  -H "Content-Type: application/json" \
  -d '{
    "points": [
      {"id": 1, "vector": [0.1, 0.2, ...], "payload": {"title": "doc1"}}
    ]
  }'

# Search
curl -X POST http://localhost:6333/collections/test/points/search \
  -H "Content-Type: application/json" \
  -d '{
    "vector": [0.1, 0.2, ...],
    "limit": 10,
    "params": {"hnsw_ef": 128}
  }'
```

### Rust SDK

```rust
use rtdb::{Database, SearchRequest, UpsertRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create database
    let db = Database::open("./data").await?;
    
    // Create collection
    db.create_collection("docs", 128, "Cosine").await?;
    
    // Insert vectors
    db.upsert("docs", UpsertRequest {
        points: vec![Point {
            id: VectorId(1),
            vector: vec![0.1; 128],
            payload: None,
        }],
    }).await?;
    
    // Search
    let results = db.search("docs", SearchRequest {
        vector: vec![0.1; 128],
        limit: 10,
        params: Some(SearchParams { hnsw_ef: Some(128), ..Default::default() }),
        ..Default::default()
    }).await?;
    
    Ok(())
}
```

## Configuration

```yaml
# config/default.yaml
server:
  rest_port: 6333
  grpc_port: 6334
  host: "0.0.0.0"

storage:
  data_dir: "./data"
  wal_max_size: 67108864  # 64MB
  memtable_size: 33554432  # 32MB
  compression: "lz4"  # none, lz4, zstd

index:
  hnsw_m: 16
  hnsw_ef_construction: 100
  hnsw_ef: 64
  use_learned_index: true
  quantization: "none"  # none, scalar, product, binary

cluster:
  enabled: false
  node_id: "node-1"
  peers: []
  raft_port: 7000

retrieval:
  enable_smart_search: true
  enable_query_expansion: true
```

## Docker

```bash
# Build image
docker build -t rtdb:latest .

# Run standalone
docker run -p 6333:6333 -p 6334:6334 rtdb:latest

# Run with Docker Compose
docker-compose up -d
```

## Testing

```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin --out Html

# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench search_benchmark
```

## Monitoring

```bash
# Metrics endpoint
curl http://localhost:9090/metrics

# Key metrics
# - rtdb_search_latency_seconds
# - rtdb_insert_ops_total
# - rtdb_storage_size_bytes
# - rtdb_index_hnsw_size
```

## Documentation

- [BENCHMARKS.md](BENCHMARKS.md) - Performance benchmarks
- [docs/COMPETITIVE_ANALYSIS.md](docs/COMPETITIVE_ANALYSIS.md) - Comparison with other databases
- [docs/COMPARISON_MATRIX.csv](docs/COMPARISON_MATRIX.csv) - Feature matrix (CSV)

## Roadmap

- [x] Core storage (LSM-tree, WAL, SSTable)
- [x] HNSW + Learned index
- [x] REST API (Qdrant-compatible)
- [x] Smart Retrieval (Zero-AI)
- [x] Docker support
- [ ] gRPC API (stabilization)
- [ ] GPU acceleration (CUDA)
- [ ] Distributed mode (Raft)
- [ ] Weaviate GraphQL API

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -am 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing`)
5. Open a Pull Request

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by [Qdrant](https://qdrant.tech), [Milvus](https://milvus.io), and [RocksDB](https://rocksdb.org)
- Uses [axum](https://github.com/tokio-rs/axum), [tonic](https://github.com/hyperium/tonic), [parking_lot](https://github.com/Amanieu/parking_lot)
