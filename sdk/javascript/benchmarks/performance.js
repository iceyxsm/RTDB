/**
 * Performance Benchmarks for RTDB JavaScript SDK
 */

const { RTDBClient, VectorUtils, PointBuilder, BatchHelper, PerformanceUtils } = require('../dist/index.js');

// Configuration
const RTDB_URL = process.env.RTDB_URL || 'http://localhost:6333';
const RTDB_API_KEY = process.env.RTDB_API_KEY;
const VECTOR_DIM = 384;
const COLLECTION_NAME = `benchmark_${Date.now()}`;

async function runBenchmarks() {
  console.log('🚀 RTDB JavaScript SDK Performance Benchmarks\n');
  
  const client = new RTDBClient({
    url: RTDB_URL,
    apiKey: RTDB_API_KEY,
    timeout: 60000
  });

  try {
    // Setup
    console.log('📋 Setup');
    console.log(`   Server: ${RTDB_URL}`);
    console.log(`   Vector Dimension: ${VECTOR_DIM}`);
    console.log(`   Collection: ${COLLECTION_NAME}\n`);

    // Check server health
    const isReady = await client.isReady();
    if (!isReady) {
      throw new Error('RTDB server is not ready');
    }

    // Create collection
    await client.createCollection(COLLECTION_NAME, {
      vector_size: VECTOR_DIM,
      distance: 'Cosine'
    });

    console.log('✅ Collection created\n');

    // Benchmark 1: Single Point Operations
    console.log('📊 Benchmark 1: Single Point Operations');
    
    const singlePoint = new PointBuilder()
      .id('benchmark-single')
      .vector(VectorUtils.random(VECTOR_DIM))
      .addPayload('type', 'benchmark')
      .build();

    const singleUpsert = await PerformanceUtils.benchmark(
      'Single Point Upsert',
      async () => {
        await client.upsert(COLLECTION_NAME, { points: [singlePoint] });
      },
      10
    );

    console.log(`   ${singleUpsert.name}: ${singleUpsert.avgDuration.toFixed(2)}ms avg (${singleUpsert.minDuration.toFixed(2)}-${singleUpsert.maxDuration.toFixed(2)}ms)`);

    const singleSearch = await PerformanceUtils.benchmark(
      'Single Vector Search',
      async () => {
        await client.search(COLLECTION_NAME, {
          vector: VectorUtils.random(VECTOR_DIM),
          limit: 10
        });
      },
      50
    );

    console.log(`   ${singleSearch.name}: ${singleSearch.avgDuration.toFixed(2)}ms avg (${singleSearch.minDuration.toFixed(2)}-${singleSearch.maxDuration.toFixed(2)}ms)`);

    // Benchmark 2: Batch Operations
    console.log('\n📊 Benchmark 2: Batch Operations');

    const batchSizes = [10, 100, 1000];
    
    for (const batchSize of batchSizes) {
      const batchPoints = Array.from({ length: batchSize }, (_, i) =>
        new PointBuilder()
          .id(`batch-${batchSize}-${i}`)
          .vector(VectorUtils.random(VECTOR_DIM))
          .addPayload('batch_size', batchSize)
          .addPayload('index', i)
          .build()
      );

      const batchUpsert = await PerformanceUtils.benchmark(
        `Batch Upsert (${batchSize} points)`,
        async () => {
          await client.upsert(COLLECTION_NAME, { points: batchPoints });
        },
        batchSize <= 100 ? 5 : 1
      );

      const throughput = (batchSize / batchUpsert.avgDuration * 1000).toFixed(0);
      console.log(`   ${batchUpsert.name}: ${batchUpsert.avgDuration.toFixed(2)}ms avg (${throughput} points/sec)`);
    }

    // Wait for indexing
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Benchmark 3: Search Performance
    console.log('\n📊 Benchmark 3: Search Performance');

    const searchLimits = [1, 10, 100];
    
    for (const limit of searchLimits) {
      const searchBench = await PerformanceUtils.benchmark(
        `Search (limit=${limit})`,
        async () => {
          await client.search(COLLECTION_NAME, {
            vector: VectorUtils.random(VECTOR_DIM),
            limit
          });
        },
        20
      );

      console.log(`   ${searchBench.name}: ${searchBench.avgDuration.toFixed(2)}ms avg (${searchBench.minDuration.toFixed(2)}-${searchBench.maxDuration.toFixed(2)}ms)`);
    }

    // Benchmark 4: Concurrent Operations
    console.log('\n📊 Benchmark 4: Concurrent Operations');

    const concurrencyLevels = [1, 5, 10, 20];
    
    for (const concurrency of concurrencyLevels) {
      const start = Date.now();
      
      const promises = Array.from({ length: concurrency }, () =>
        client.search(COLLECTION_NAME, {
          vector: VectorUtils.random(VECTOR_DIM),
          limit: 10
        })
      );

      await Promise.all(promises);
      
      const duration = Date.now() - start;
      const avgLatency = duration / concurrency;
      const qps = (concurrency / duration * 1000).toFixed(1);
      
      console.log(`   Concurrency ${concurrency}: ${avgLatency.toFixed(2)}ms avg latency, ${qps} QPS`);
    }

    // Benchmark 5: Batch Search
    console.log('\n📊 Benchmark 5: Batch Search');

    const batchSearchSizes = [1, 5, 10, 20];
    
    for (const batchSize of batchSearchSizes) {
      const searches = Array.from({ length: batchSize }, () => ({
        vector: VectorUtils.random(VECTOR_DIM),
        limit: 5
      }));

      const batchSearchBench = await PerformanceUtils.benchmark(
        `Batch Search (${batchSize} queries)`,
        async () => {
          await client.searchBatch(COLLECTION_NAME, { searches });
        },
        10
      );

      const avgPerQuery = batchSearchBench.avgDuration / batchSize;
      console.log(`   ${batchSearchBench.name}: ${batchSearchBench.avgDuration.toFixed(2)}ms total, ${avgPerQuery.toFixed(2)}ms per query`);
    }

    // Benchmark 6: Memory and Vector Operations
    console.log('\n📊 Benchmark 6: Vector Operations');

    const vectorA = VectorUtils.random(VECTOR_DIM);
    const vectorB = VectorUtils.random(VECTOR_DIM);

    const vectorOps = [
      {
        name: 'Cosine Similarity',
        fn: () => VectorUtils.cosineSimilarity(vectorA, vectorB)
      },
      {
        name: 'Euclidean Distance',
        fn: () => VectorUtils.euclideanDistance(vectorA, vectorB)
      },
      {
        name: 'Dot Product',
        fn: () => VectorUtils.dotProduct(vectorA, vectorB)
      },
      {
        name: 'Vector Normalization',
        fn: () => VectorUtils.normalize(vectorA)
      }
    ];

    for (const op of vectorOps) {
      const bench = await PerformanceUtils.benchmark(
        op.name,
        async () => op.fn(),
        10000
      );

      const opsPerSec = (1000 / bench.avgDuration).toFixed(0);
      console.log(`   ${bench.name}: ${bench.avgDuration.toFixed(4)}ms avg (${opsPerSec}K ops/sec)`);
    }

    // Benchmark 7: Large Dataset Performance
    console.log('\n📊 Benchmark 7: Large Dataset Performance');

    // Insert a larger dataset
    const largeDatasetSize = 10000;
    const chunks = BatchHelper.chunk(
      Array.from({ length: largeDatasetSize }, (_, i) =>
        new PointBuilder()
          .id(`large-${i}`)
          .vector(VectorUtils.random(VECTOR_DIM))
          .addPayload('dataset', 'large')
          .addPayload('index', i)
          .build()
      ),
      1000
    );

    console.log(`   Inserting ${largeDatasetSize} points in ${chunks.length} chunks...`);
    
    const largeInsertStart = Date.now();
    for (let i = 0; i < chunks.length; i++) {
      await client.upsert(COLLECTION_NAME, { points: chunks[i] });
      if (i % 2 === 0) {
        process.stdout.write(`\r   Progress: ${Math.round((i + 1) / chunks.length * 100)}%`);
      }
    }
    const largeInsertDuration = Date.now() - largeInsertStart;
    
    console.log(`\n   Large dataset insert: ${largeInsertDuration}ms (${(largeDatasetSize / largeInsertDuration * 1000).toFixed(0)} points/sec)`);

    // Wait for indexing
    console.log('   Waiting for indexing...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    // Search performance on large dataset
    const largeSearchBench = await PerformanceUtils.benchmark(
      'Search on Large Dataset',
      async () => {
        await client.search(COLLECTION_NAME, {
          vector: VectorUtils.random(VECTOR_DIM),
          limit: 10
        });
      },
      20
    );

    console.log(`   ${largeSearchBench.name}: ${largeSearchBench.avgDuration.toFixed(2)}ms avg`);

    // Count performance
    const countBench = await PerformanceUtils.benchmark(
      'Count Operation',
      async () => {
        await client.count(COLLECTION_NAME);
      },
      10
    );

    console.log(`   ${countBench.name}: ${countBench.avgDuration.toFixed(2)}ms avg`);

    // Final statistics
    console.log('\n📈 Final Statistics');
    const finalCount = await client.count(COLLECTION_NAME);
    const collectionInfo = await client.getCollection(COLLECTION_NAME);
    
    console.log(`   Total Points: ${finalCount}`);
    console.log(`   Vectors Count: ${collectionInfo.vectors_count}`);
    console.log(`   Segments: ${collectionInfo.segments_count}`);
    console.log(`   Status: ${collectionInfo.status}`);

    // Performance Summary
    console.log('\n🎯 Performance Summary');
    console.log(`   Single Search Latency: ${singleSearch.avgDuration.toFixed(2)}ms`);
    console.log(`   Batch Insert Throughput: ${(1000 / batchUpsert.avgDuration * 1000).toFixed(0)} points/sec`);
    console.log(`   Large Dataset Search: ${largeSearchBench.avgDuration.toFixed(2)}ms`);
    console.log(`   Vector Operations: ${(1000 / vectorOps[0].fn().avgDuration).toFixed(0)}K ops/sec`);

  } catch (error) {
    console.error('❌ Benchmark failed:', error.message);
    if (error.status) {
      console.error('   Status:', error.status);
    }
  } finally {
    // Cleanup
    try {
      await client.deleteCollection(COLLECTION_NAME);
      console.log('\n🧹 Cleanup completed');
    } catch (error) {
      console.error('⚠️  Cleanup failed:', error.message);
    }
    
    client.close();
  }
}

// Memory usage monitoring
function logMemoryUsage() {
  if (typeof process !== 'undefined' && process.memoryUsage) {
    const usage = process.memoryUsage();
    console.log('\n💾 Memory Usage:');
    console.log(`   RSS: ${Math.round(usage.rss / 1024 / 1024)}MB`);
    console.log(`   Heap Used: ${Math.round(usage.heapUsed / 1024 / 1024)}MB`);
    console.log(`   Heap Total: ${Math.round(usage.heapTotal / 1024 / 1024)}MB`);
    console.log(`   External: ${Math.round(usage.external / 1024 / 1024)}MB`);
  }
}

// Run benchmarks
if (require.main === module) {
  console.log('Starting RTDB JavaScript SDK Benchmarks...\n');
  
  runBenchmarks()
    .then(() => {
      logMemoryUsage();
      console.log('\n✅ All benchmarks completed successfully!');
    })
    .catch(error => {
      console.error('\n❌ Benchmarks failed:', error);
      process.exit(1);
    });
}

module.exports = { runBenchmarks };