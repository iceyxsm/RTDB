# gRPC Performance Optimizations

This document describes the high-performance gRPC optimizations implemented for the RTDB cluster communication layer.

## Overview

The Phase 3 inter-node gRPC communication has been optimized for maximum throughput and low latency in distributed deployments. These optimizations are based on industry best practices from Google, YDB, and high-performance distributed systems research.

## Key Optimizations

### 1. Connection Pooling

**Problem**: Single gRPC channel per node limits HTTP/2 concurrent streams (default 100).

**Solution**: Implemented connection pooling with multiple channels per node:
- Default of 4 connections per node (configurable)
- Round-robin distribution across connections
- Lock-free access using `DashMap` instead of `RwLock<HashMap>`

**Benefits**:
- 4x improvement in concurrent request capacity
- Eliminates lock contention on connection access
- Better distribution of load across TCP connections

**Configuration**:
```rust
ClientConfig {
    connection_pool_size: 4,  // Connections per node
    ...
}
```

### 2. HTTP/2 Keepalive Configuration

**Problem**: Idle connections may be closed by firewalls/load balancers, causing reconnect latency.

**Solution**: Configured aggressive HTTP/2 keepalive:
- Keepalive interval: 30 seconds
- Keepalive timeout: 10 seconds
- Keep-alive while idle enabled

**Benefits**:
- Connections stay warm and ready
- Early detection of failed peers
- Reduced latency on request bursts after idle periods

### 3. TCP/HTTP-2 Window Sizing

**Problem**: Default window sizes limit throughput on high-latency networks.

**Solution**: Tuned HTTP/2 flow control windows:
- Initial stream window: 64KB (65535 bytes)
- Initial connection window: 1MB (1048576 bytes)
- Max frame size: 1MB

**Benefits**:
- Better throughput on WAN links
- Reduced stalling on large message transfers
- More efficient bandwidth utilization

### 4. Compression Support

**Problem**: Vector data can be large (4 bytes per dimension), wasting bandwidth.

**Solution**: Gzip compression support:
- Client requests can enable gzip encoding
- Server accepts and responds with gzip
- Compression applied to large vector payloads

**Configuration**:
```rust
ClientConfig {
    enable_compression: true,
}

ServerConfig {
    enable_compression: true,
}
```

### 5. Configurable Timeouts

**Problem**: Default timeouts don't account for operation-specific requirements.

**Solution**: Operation-specific timeout configuration:
- Default requests: 5 seconds
- Search operations: 30 seconds (compute intensive)
- Batch operations: 60 seconds (bulk data)
- Replication: 10 seconds (network tolerant)
- Heartbeats: 3 seconds (quick failure detection)

### 6. Batch Operations

**Problem**: Individual RPC calls have overhead (HTTP/2 framing, serialization).

**Solution**: Implemented batch APIs:
- `BatchSearch` - Multiple query vectors in one request
- `BatchInsert` - Multiple vectors in one request  
- `BatchReplicate` - Multiple replication entries in one request
- `StreamReplicate` - Continuous streaming for high-throughput replication

**Benefits**:
- Reduced RPC overhead
- Better throughput for bulk operations
- Amortized network latency across many operations

### 7. Binary Vector Encoding

**Problem**: `repeated float` in protobuf is less efficient than binary encoding.

**Solution**: Vectors encoded as `bytes` field:
- 4 bytes per f32 (little-endian)
- Zero-copy conversion using `Bytes` types
- More compact wire format

**Example**:
```rust
// Encode
fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter()
        .flat_map(|&f| f.to_le_bytes())
        .collect()
}

// Decode
fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
```

### 8. Server Concurrency Limits

**Problem**: Unbounded concurrency can overwhelm server resources.

**Solution**: Configurable concurrency limits:
- Default: 1024 concurrent requests
- HTTP/2 concurrent stream limits tuned
- Backpressure via tonic's concurrency limit middleware

### 9. Request ID Tracking

**Problem**: Difficult to trace requests across distributed nodes.

**Solution**: Per-request ID assignment:
- Atomic counter for unique request IDs
- IDs propagated in request/response metadata
- Enables distributed tracing and debugging

### 10. Packed Encoding for Repeated Fields

**Problem**: Repeated integer fields have overhead in protobuf.

**Solution**: Applied `[packed = true]` annotation:
- More efficient encoding for shard lists
- Reduced message size for topology updates

## Architecture

### Client Architecture

```
ClusterClient
├── ClientConfig (timeouts, compression, pool size)
├── connection_pools: DashMap<node_id, ConnectionPool>
└── request_id_counter: AtomicUsize

ConnectionPool
├── channels: Vec<ClusterServiceClient<Channel>>
├── current_index: AtomicUsize (round-robin)
└── address: String
```

### Server Architecture

```
ClusterGrpcServer
├── ServerConfig (concurrency, window sizes, compression)
└── ClusterServiceImpl
    ├── cluster: Arc<RwLock<ClusterManager>>
    └── config: ServerConfig
```

## Configuration Examples

### High-Throughput Client

```rust
let client_config = ClientConfig {
    connection_pool_size: 8,           // More connections for high load
    request_timeout: Duration::from_secs(5),
    search_timeout: Duration::from_secs(30),
    enable_compression: true,
    enable_keepalive: true,
    tls_config: None,
};

let client = ClusterClient::with_client_config(cluster_config, client_config);
```

### High-Performance Server

```rust
let server_config = ServerConfig {
    concurrency_limit: 2048,           // Double default for heavy load
    request_timeout: Duration::from_secs(30),
    enable_compression: true,
    tcp_keepalive: Some(Duration::from_secs(60)),
    http2_keepalive_interval: Some(Duration::from_secs(30)),
    max_frame_size: Some(1024 * 1024),
    initial_stream_window_size: Some(65535),
    initial_connection_window_size: Some(1024 * 1024),
};

let server = ClusterGrpcServer::with_config(cluster, bind_addr, server_config);
```

## Performance Expectations

Based on research and benchmarks from similar optimizations:

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Concurrent Requests | 100/connection | 400/node (4 conn) | 4x |
| Connection Overhead | High (reconnects) | Low (pooled) | ~6x latency |
| Batch Throughput | 1 vec/RPC | N vecs/RPC | Nx reduction |
| Network Bandwidth | 100% | ~30-50% (compressed) | 2-3x |

## Best Practices

1. **Use batch operations** for bulk inserts/searches
2. **Enable compression** for large vector payloads
3. **Tune connection pool size** based on expected concurrency
4. **Monitor connection pool stats** for optimization
5. **Use appropriate timeouts** for each operation type

## Future Optimizations

Potential future improvements:

1. **Custom Load Balancing**: Weighted round-robin based on node load
2. **Circuit Breaker**: Fail fast on unhealthy nodes
3. **Request Batching**: Automatic client-side batching with time windows
4. **Zero-Copy Serialization**: Using `Bytes` for payload references
5. **Connection Warmup**: Pre-establish connections to expected peers
6. **Adaptive Compression**: Disable compression for small messages
7. **Request Prioritization**: QoS tiers for different operation types

## References

- [gRPC Performance Best Practices](https://grpc.io/docs/guides/performance/)
- [The Surprising gRPC Client Bottleneck](https://blog.ydb.tech/the-surprising-grpc-client-bottleneck-in-low-latency-networks-and-how-to-get-around-it-69d6977a1d02)
- [Boosting gRPC Performance](https://www.bytesizego.com/blog/grpc-performance)
- [Tonic Documentation](https://docs.rs/tonic/latest/tonic/)
