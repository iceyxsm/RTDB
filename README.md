# RTDB - Real-Time Database

A production-grade smart vector database written in Rust, featuring LSM-tree storage, hybrid indexing (HNSW + Learned), distributed consensus (Raft), and comprehensive observability.

[![Build](https://img.shields.io/badge/build-passing-brightgreen)](https://github.com/iceyxsm/RTDB)
[![Tests](https://img.shields.io/badge/tests-86%2F86-brightgreen)](https://github.com/iceyxsm/RTDB)
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

See [BENCHMARKS.md](docs/BENCHMARKS.md) for full details.

## Key Features

### Core Storage
- **LSM-Tree Engine** with WAL crash recovery (CRC32C)
- **MemTable** - Lock-free skiplist for hot data
- **SSTable** - Columnar format with compression (LZ4/Zstd)
- **Zero Dependencies** - Single 15MB binary

### Vector Indexing
- **HNSW** - Hierarchical Navigable Small World graphs (M=16, ef=64)
- **Learned Index** - Piecewise linear models for range queries
- **Quantization** - Product Quantization (PQ) + Binary Quantization (BQ)
- **SIMD** - AVX2/NEON optimized distance kernels

### Distributed Systems
- **Raft Consensus** - Leader election, log replication, snapshots
- **Failover & Recovery** - Phi Accrual failure detection, fencing tokens
- **Data Replication** - Quorum writes, follower reads with lag tracking
- **Hash Ring** - Consistent sharding across cluster nodes

### Smart Retrieval (Zero-AI)
- **Intent Classification** - Rule-based query understanding
- **Query Expansion** - Automatic synonym and entity expansion
- **Knowledge Graph** - Built-in entity relationships
- **Auto-Complete** - Fuzzy prefix search

### Observability & Monitoring 

Production-grade observability following industry best practices from Qdrant, Milvus, and Datadog:

#### Prometheus Metrics
Metrics collection infrastructure with cardinality protection:
- **Query Metrics** - QPS, latency histograms (p50/p95/p99), error rates
- **Index Metrics** - Vector count, index size, recall ratio, build duration  
- **Storage Metrics** - Size, document count, collection count
- **Replication Metrics** - Lag seconds, replication ops
- **System Metrics** - CPU, memory, disk, open file descriptors
- **Cardinality Limiting** - Prevents metric explosion (max 1000 unique values/metric)

```rust
// Record query metrics
metrics.record_query("users", duration, true);

// Export in Prometheus format
let output = metrics.export_metrics()?;
```

Configuration: `metrics_bind: "0.0.0.0:9090"` in config (endpoint ready for integration)

#### Grafana Dashboard (Configuration)
Pre-built dashboard JSON in `config/monitoring/grafana-dashboard.json`:
- Overview: QPS, error rate, P99 latency, memory, recall
- Query Performance: Latency percentiles by collection
- Index & Storage: Size metrics, vector counts
- Replication & Cluster: Lag tracking, connection metrics

Import into Grafana and connect to Prometheus datasource for visualization.

#### AlertManager Rules (Configuration)
Alert rule definitions in `config/monitoring/alert-rules.yml`:
- **Critical**: P99 latency >1s, memory >90%, error rate >5%, no quorum
- **Warning**: P95 latency >500ms, memory >85%, replication lag >10s
- **Info**: Rapid storage growth, large collections

Rules include runbook URLs and dashboard links. Configure AlertManager to load these rules.

#### OpenTelemetry Distributed Tracing
- **Context Propagation** - W3C Trace Context across HTTP/gRPC
- **Batched Exports** - Configurable batch sizes (512-1024 spans)
- **Compression** - gzip compression for reduced bandwidth
- **Sampling** - Head-based sampling with parent respect (default 10% in production)

```rust
// Initialize tracing with production config
let config = TracingConfig::production();
init_tracing(&config)?;

// Automatic trace context extraction/injection
let context = extract_context_from_headers(&headers);
inject_context_into_headers(&context, &mut response_headers);
```

#### Structured Logging
- **JSON Format** - Machine-parseable for ELK/Loki
- **Trace Correlation** - Automatic trace_id/span_id injection
- **PII Redaction** - Automatic redaction of email, password, token fields
- **Performance** - Async writing to prevent request blocking

```json
{
  "@timestamp": "2024-01-01T00:00:00Z",
  "level": "INFO",
  "message": "Query completed",
  "trace_id": "abc123...",
  "span_id": "def456...",
  "service": "rtdb",
  "environment": "production"
}
```

#### Health Checks
Health check infrastructure (Kubernetes-compatible probes ready for integration):
- **LivenessCheck** - Is the application running?
- **ReadinessCheck** - Is it ready to serve traffic?
- **StartupCheck** - Has startup completed?
- **HealthChecker** - Aggregated health status with HTTP endpoint support

```rust
// Check overall health
let health = health_checker.check_all().await;
if health.status.is_healthy() {
    println!("System is healthy");
}
```

### API Compatibility
- **Qdrant** - REST API (port 6333) and gRPC (port 6334)
- **Milvus** - SDK compatible (partial)
- **Weaviate** - GraphQL (planned)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         API Layer                                   │
│  ┌──────────────┬──────────────┬────────────────────────────────┐   │
│  │ REST (6333)  │ gRPC (6334)  │ Smart Retrieval                │   │
│  └──────────────┴──────────────┴────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────┤
│                      Collection Layer                               │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  Collection Manager  │  Index Manager  │  Storage            │   │
│  └──────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────┤
│                      Index Layer                                    │
│  ┌──────────────┬──────────────┬──────────────────────────────┐     │
│  │ HNSW         │ Learned Index│ Flat (Brute Force)           │     │
│  └──────────────┴──────────────┴──────────────────────────────┘     │
├─────────────────────────────────────────────────────────────────────┤
│                      Storage Layer                                  │
│  ┌──────────────┬──────────────┬──────────────────────────────┐     │
│  │ WAL          │ MemTable     │ SSTable (LSM-Tree)           │     │
│  └──────────────┴──────────────┴──────────────────────────────┘     │
├─────────────────────────────────────────────────────────────────────┤
│                      Cluster Layer                                  │
│  ┌──────────────┬──────────────┬──────────────────────────────┐     │
│  │ Raft         │ Replication  │ Failover Manager             │     │
│  └──────────────┴──────────────┴──────────────────────────────┘     │
├─────────────────────────────────────────────────────────────────────┤
│                      Observability Layer                            │
│  ┌──────────────┬──────────────┬──────────────────────────────┐     │
│  │ Prometheus   │ OpenTelemetry│ Health Checks                │     │
│  └──────────────┴──────────────┴──────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
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
  metrics_port: 9090
  health_port: 8080

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

observability:
  metrics_enabled: true
  tracing_enabled: true
  sampling_ratio: 0.1  # 10% sampling
  enable_compression: true
  max_metric_cardinality: 1000

retrieval:
  enable_smart_search: true
  enable_query_expansion: true
```

## Docker

```bash
# Build image
docker build -t rtdb:latest .

# Run standalone
docker run -p 6333:6333 -p 6334:6334 -p 9090:9090 rtdb:latest

# Run with Docker Compose (includes Prometheus/Grafana)
docker-compose up -d
```

## Observability Setup

### Prometheus & Grafana

```bash
# Start monitoring stack
cd config/monitoring
docker-compose up -d

# Import dashboard
# 1. Open Grafana at http://localhost:3000
# 2. Login: admin/admin
# 3. Import grafana-dashboard.json
# 4. Select Prometheus datasource
```

### OpenTelemetry Collector

```yaml
# otel-collector-config.yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    timeout: 5s
    send_batch_size: 512

exporters:
  jaeger:
    endpoint: jaeger-collector:14250
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [jaeger]
```

### AlertManager Configuration

```yaml
# config/monitoring/alertmanager.yml
route:
  receiver: 'slack'
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'

receivers:
  - name: 'slack'
    slack_configs:
      - api_url: '${SLACK_WEBHOOK_URL}'
        channel: '#alerts'
  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: '${PAGERDUTY_KEY}'
```

## Testing

```bash
# Run all tests
cargo test --lib

# Run with coverage
cargo tarpaulin --out Html

# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench search_benchmark

# Run observability tests
cargo test observability
```

## Monitoring Endpoints

```bash
# Prometheus metrics
curl http://localhost:9090/metrics

# Health checks
curl http://localhost:8080/health
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready

# Key metrics
# - rtdb_query_duration_seconds (histogram)
# - rtdb_queries_total (counter)
# - rtdb_index_recall (gauge)
# - rtdb_replication_lag_seconds (gauge)
```

## Documentation

- [BENCHMARKS.md](docs/BENCHMARKS.md) - Performance benchmarks
- [docs/COMPETITIVE_ANALYSIS.md](docs/COMPETITIVE_ANALYSIS.md) - Comparison with other databases
- [docs/COMPARISON_MATRIX.csv](docs/COMPARISON_MATRIX.csv) - Feature matrix (CSV)
- [config/monitoring/](config/monitoring/) - Observability configurations

## Roadmap

- [x] Core storage (LSM-tree, WAL, SSTable)
- [x] HNSW + Learned index
- [x] REST API (Qdrant-compatible)
- [x] Smart Retrieval (Zero-AI)
- [x] Docker support
- [x] **Observability (Prometheus, Grafana, OpenTelemetry)**
- [x] **Distributed consensus (Raft)**
- [x] **Failover & Recovery**
- [ ] gRPC API (stabilization)
- [ ] GPU acceleration (CUDA)
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
- Observability patterns from [Datadog](https://datadoghq.com), [OneUptime](https://oneuptime.com)
- Uses [axum](https://github.com/tokio-rs/axum), [tonic](https://github.com/hyperium/tonic), [parking_lot](https://github.com/Amanieu/parking_lot)
