# RTDB JavaScript/TypeScript SDK

Official JavaScript/TypeScript client library for RTDB vector database with full Qdrant API compatibility.

[![npm version](https://badge.fury.io/js/@rtdb/client.svg)](https://badge.fury.io/js/@rtdb/client)
[![TypeScript](https://img.shields.io/badge/TypeScript-Ready-blue.svg)](https://www.typescriptlang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- **High Performance**: Sub-5ms P99 latency with HTTP/2 and connection pooling
- **Full Qdrant Compatibility**: Drop-in replacement for Qdrant JavaScript client
- **TypeScript First**: Complete type definitions with IntelliSense support
- **Production Ready**: Retry logic, error handling, and comprehensive validation
- **Developer Friendly**: Fluent APIs, utility functions, and extensive examples
- **Universal**: Works in Node.js, browsers, and edge environments
- **Optimized**: Connection pooling, request batching, and smart caching

## Installation

```bash
npm install @rtdb/client
```

```bash
yarn add @rtdb/client
```

```bash
pnpm add @rtdb/client
```

## Quick Start

```typescript
import { RTDBClient, PointBuilder, VectorUtils } from '@rtdb/client';

// Create client
const client = new RTDBClient({
  url: 'http://localhost:6333',
  apiKey: 'your-api-key' // Optional
});

// Create collection
await client.createCollection('my_collection', {
  vector_size: 128,
  distance: 'Cosine'
});

// Insert points
const points = [
  new PointBuilder()
    .id('doc-1')
    .vector(VectorUtils.random(128))
    .addPayload('title', 'First document')
    .addPayload('category', 'tech')
    .build()
];

await client.upsert('my_collection', { points });

// Search
const results = await client.search('my_collection', {
  vector: VectorUtils.random(128),
  limit: 10,
  with_payload: true
});

console.log('Search results:', results.result);
```

## Configuration

### Client Options

```typescript
const client = new RTDBClient({
  url: 'http://localhost:6333',           // RTDB server URL
  apiKey: 'your-api-key',                 // Optional API key
  timeout: 30000,                         // Request timeout (ms)
  retries: 3,                             // Number of retries
  retryDelay: 100,                        // Initial retry delay (ms)
  maxRetryDelay: 5000,                    // Maximum retry delay (ms)
  retryMultiplier: 2,                     // Retry delay multiplier
  headers: {                              // Custom headers
    'Custom-Header': 'value'
  }
});
```

### Collection Configuration

```typescript
import { ConfigHelper } from '@rtdb/client';

await client.createCollection('my_collection', {
  vector_size: 768,
  distance: 'Cosine',
  hnsw_config: ConfigHelper.accurateHnsw(),
  quantization_config: ConfigHelper.scalarQuantization(0.95),
  on_disk_payload: false
});
```

## Core Operations

### Collection Management

```typescript
// Create collection
await client.createCollection('docs', {
  vector_size: 384,
  distance: 'Cosine'
});

// List collections
const collections = await client.listCollections();

// Get collection info
const info = await client.getCollection('docs');

// Delete collection
await client.deleteCollection('docs');
```

### Point Operations

```typescript
import { PointBuilder } from '@rtdb/client';

// Create points
const points = [
  new PointBuilder()
    .id('point-1')
    .vector([0.1, 0.2, 0.3, 0.4])
    .addPayload('category', 'tech')
    .addPayload('score', 0.9)
    .build()
];

// Insert/update points
await client.upsert('docs', { points });

// Get point by ID
const point = await client.getPoint('docs', 'point-1');

// Retrieve multiple points
const retrieved = await client.retrieve('docs', {
  ids: ['point-1', 'point-2'],
  with_payload: true
});

// Delete points
await client.delete('docs', {
  points: ['point-1', 'point-2']
});
```

### Vector Search

```typescript
// Basic search
const results = await client.search('docs', {
  vector: [0.1, 0.2, 0.3, 0.4],
  limit: 10,
  with_payload: true
});

// Search with filters
import { FilterBuilder } from '@rtdb/client';

const filter = new FilterBuilder()
  .equals('category', 'tech')
  .range('score', { gte: 0.8 })
  .build();

const filtered = await client.search('docs', {
  vector: [0.1, 0.2, 0.3, 0.4],
  limit: 10,
  filter,
  score_threshold: 0.7
});

// Batch search
const batchResults = await client.searchBatch('docs', {
  searches: [
    { vector: [0.1, 0.2, 0.3, 0.4], limit: 5 },
    { vector: [0.5, 0.6, 0.7, 0.8], limit: 5 }
  ]
});
```

## Advanced Features

### Complex Filtering

```typescript
import { FilterBuilder } from '@rtdb/client';

const complexFilter = new FilterBuilder()
  .should(
    new FilterBuilder()
      .equals('category', 'tech')
      .range('score', { gte: 0.8 })
      .build()
  )
  .should(
    new FilterBuilder()
      .equals('priority', 'high')
      .build()
  )
  .mustNot(
    new FilterBuilder()
      .equals('status', 'archived')
      .build()
  )
  .build();

const results = await client.search('docs', {
  vector: queryVector,
  filter: complexFilter,
  limit: 20
});
```

### Batch Operations

```typescript
import { BatchHelper } from '@rtdb/client';

// Create large batches efficiently
const vectors = Array.from({ length: 10000 }, () => VectorUtils.random(384));
const points = BatchHelper.createPoints(vectors);

// Insert in chunks
const chunks = BatchHelper.chunk(points, 100);
for (const chunk of chunks) {
  await client.upsert('docs', { points: chunk });
}
```

### Performance Monitoring

```typescript
import { PerformanceUtils } from '@rtdb/client';

// Measure operation time
const { result, duration } = await PerformanceUtils.measureTime(async () => {
  return await client.search('docs', { vector: queryVector, limit: 10 });
});

console.log(`Search took ${duration.toFixed(2)}ms`);

// Benchmark operations
const benchmark = await PerformanceUtils.benchmark(
  'Search Operation',
  () => client.search('docs', { vector: queryVector, limit: 10 }),
  100 // iterations
);

console.log(`Average: ${benchmark.avgDuration.toFixed(2)}ms`);
```

### Rate Limiting

```typescript
import { PerformanceUtils } from '@rtdb/client';

const rateLimiter = PerformanceUtils.createRateLimiter(10); // 10 requests/second

for (const query of queries) {
  await rateLimiter();
  const results = await client.search('docs', query);
  // Process results...
}
```

## Vector Utilities

```typescript
import { VectorUtils } from '@rtdb/client';

// Generate random vectors
const randomVector = VectorUtils.random(384, -1, 1);

// Calculate similarities
const similarity = VectorUtils.cosineSimilarity(vectorA, vectorB);
const distance = VectorUtils.euclideanDistance(vectorA, vectorB);

// Vector operations
const normalized = VectorUtils.normalize(vector);
const sum = VectorUtils.add(vectorA, vectorB);
const scaled = VectorUtils.scale(vector, 2.0);
```

## Error Handling

```typescript
import { RTDBError, ConnectionError, ValidationError } from '@rtdb/client';

try {
  await client.search('docs', { vector: queryVector });
} catch (error) {
  if (error instanceof ValidationError) {
    console.error('Validation error:', error.message);
  } else if (error instanceof ConnectionError) {
    console.error('Connection error:', error.message);
  } else if (error instanceof RTDBError) {
    console.error('RTDB error:', error.message, error.status);
  } else {
    console.error('Unknown error:', error);
  }
}
```

## TypeScript Support

The SDK is written in TypeScript and provides comprehensive type definitions:

```typescript
import type {
  CollectionConfig,
  SearchRequest,
  SearchResponse,
  Point,
  Filter
} from '@rtdb/client';

// Fully typed operations
const config: CollectionConfig = {
  vector_size: 384,
  distance: 'Cosine'
};

const searchRequest: SearchRequest = {
  vector: [0.1, 0.2, 0.3],
  limit: 10,
  with_payload: true
};

const results: SearchResponse = await client.search('docs', searchRequest);
```

## Browser Usage

The SDK works in browsers with a UMD build:

```html
<script src="https://unpkg.com/@rtdb/client/dist/rtdb-client.umd.js"></script>
<script>
  const client = new RTDBClient.RTDBClient({
    url: 'https://your-rtdb-server.com'
  });
  
  // Use the client...
</script>
```

## Examples

Check out the [examples](./examples/) directory for complete examples:

- [Basic Usage](./examples/basic-usage.js) - Getting started guide
- [Advanced Features](./examples/advanced-features.js) - Complex operations and performance optimization

## API Reference

### Client Methods

#### Collection Management
- `createCollection(name, config)` - Create a new collection
- `listCollections()` - List all collections
- `getCollection(name)` - Get collection information
- `collectionExists(name)` - Check if collection exists
- `deleteCollection(name)` - Delete a collection

#### Point Operations
- `upsert(collection, request)` - Insert or update points
- `search(collection, request)` - Search for similar vectors
- `searchBatch(collection, request)` - Batch search operations
- `query(collection, request)` - Advanced query with prefetch
- `retrieve(collection, request)` - Get points by IDs
- `getPoint(collection, id)` - Get a single point
- `delete(collection, request)` - Delete points
- `scroll(collection, request)` - Scroll through points
- `count(collection, request)` - Count points

#### Snapshot Operations
- `createSnapshot(collection)` - Create collection snapshot
- `listSnapshots(collection)` - List collection snapshots
- `deleteSnapshot(collection, name)` - Delete snapshot
- `createFullSnapshot()` - Create full database snapshot
- `listFullSnapshots()` - List full database snapshots

#### Service Operations
- `healthCheck()` - Get service health
- `isReady()` - Check if service is ready
- `isAlive()` - Check if service is alive
- `getTelemetry()` - Get telemetry data

### Utility Classes

- `VectorUtils` - Vector operations and calculations
- `FilterBuilder` - Fluent filter construction
- `PointBuilder` - Fluent point construction
- `BatchHelper` - Batch operation utilities
- `ConfigHelper` - Configuration presets
- `ValidationUtils` - Validation functions
- `PerformanceUtils` - Performance measurement and optimization

## Performance

The RTDB JavaScript SDK is optimized for high-performance applications:

- **HTTP/2 Support**: Multiplexed connections for better throughput
- **Connection Pooling**: Reuse connections to reduce latency
- **Request Batching**: Combine multiple operations efficiently
- **Retry Logic**: Exponential backoff with jitter
- **Compression**: Gzip compression for reduced bandwidth
- **Streaming**: Support for large result sets with pagination

### Benchmarks

Typical performance on modern hardware:

- **Search Latency**: <5ms P99 for 1M vectors
- **Batch Insert**: 10,000+ vectors/second
- **Memory Usage**: <50MB for 1M vectors (with quantization)
- **Concurrent Requests**: 1000+ QPS per client instance

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development

```bash
# Install dependencies
npm install

# Run tests
npm test

# Run integration tests (requires RTDB server)
RUN_INTEGRATION_TESTS=true RTDB_URL=http://localhost:6333 npm test

# Build
npm run build

# Lint
npm run lint

# Generate documentation
npm run docs
```

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Support

- 📖 [Documentation](https://github.com/iceyxsm/RTDB/tree/main/sdk/javascript)
- 🐛 [Issue Tracker](https://github.com/iceyxsm/RTDB/issues)
- 💬 [Discussions](https://github.com/iceyxsm/RTDB/discussions)

## Related Projects

- [RTDB](https://github.com/iceyxsm/RTDB) - Main RTDB vector database
- [RTDB Python SDK](https://github.com/iceyxsm/RTDB/tree/main/sdk/python) - Python client library
- [Qdrant](https://qdrant.tech) - Compatible vector database