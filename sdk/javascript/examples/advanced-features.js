/**
 * Advanced Features Example for RTDB JavaScript SDK
 */

const { 
  RTDBClient, 
  FilterBuilder, 
  PointBuilder, 
  VectorUtils, 
  BatchHelper,
  ConfigHelper,
  PerformanceUtils
} = require('@rtdb/client');

async function advancedExample() {
  const client = new RTDBClient({
    url: 'http://localhost:6333',
    timeout: 60000,
    retries: 5
  });

  try {
    console.log('=== Advanced RTDB Features Demo ===\n');

    // 1. Create collection with advanced configuration
    const collectionName = 'advanced_collection';
    await client.createCollection(collectionName, {
      vector_size: 384,
      distance: 'Cosine',
      hnsw_config: ConfigHelper.accurateHnsw(),
      quantization_config: ConfigHelper.scalarQuantization(0.95),
      on_disk_payload: false
    });
    console.log('✓ Created collection with advanced configuration');

    // 2. Batch operations with large datasets
    console.log('\n--- Batch Operations ---');
    const batchSize = 1000;
    const vectors = Array.from({ length: batchSize }, () => VectorUtils.random(384));
    const points = BatchHelper.createPoints(vectors).map((point, index) => ({
      ...point,
      payload: {
        category: ['tech', 'science', 'art', 'music'][index % 4],
        score: Math.random(),
        tags: [`tag-${index % 10}`, `group-${Math.floor(index / 100)}`],
        metadata: {
          created_at: new Date().toISOString(),
          index: index,
          is_important: index % 10 === 0
        }
      }
    }));

    // Insert in chunks to avoid overwhelming the server
    const chunks = BatchHelper.chunk(points, 100);
    console.log(`Inserting ${batchSize} points in ${chunks.length} chunks...`);

    const { duration: insertDuration } = await PerformanceUtils.measureTime(async () => {
      for (let i = 0; i < chunks.length; i++) {
        await client.upsert(collectionName, { points: chunks[i] });
        if (i % 5 === 0) {
          console.log(`  Inserted chunk ${i + 1}/${chunks.length}`);
        }
      }
    });

    console.log(`✓ Batch insert completed in ${insertDuration.toFixed(2)}ms`);
    console.log(`  Average: ${(insertDuration / batchSize).toFixed(2)}ms per point`);

    // Wait for indexing
    await new Promise(resolve => setTimeout(resolve, 2000));

    // 3. Complex filtering and search
    console.log('\n--- Complex Filtering ---');
    
    const complexFilter = new FilterBuilder()
      .should(
        new FilterBuilder()
          .equals('category', 'tech')
          .range('score', { gte: 0.7 })
          .build()
      )
      .should(
        new FilterBuilder()
          .equals('metadata.is_important', true)
          .build()
      )
      .mustNot(
        new FilterBuilder()
          .in('tags', ['tag-1', 'tag-2'])
          .build()
      )
      .build();

    const complexSearchResults = await client.search(collectionName, {
      vector: VectorUtils.random(384),
      limit: 20,
      filter: complexFilter,
      with_payload: true,
      score_threshold: 0.1
    });

    console.log(`✓ Complex search returned ${complexSearchResults.result.length} results`);
    console.log('  Sample results:');
    complexSearchResults.result.slice(0, 3).forEach((result, index) => {
      console.log(`    ${index + 1}. Score: ${result.score.toFixed(4)}, Category: ${result.payload?.category}, Important: ${result.payload?.metadata?.is_important}`);
    });

    // 4. Batch search for multiple queries
    console.log('\n--- Batch Search ---');
    const queryVectors = Array.from({ length: 5 }, () => VectorUtils.random(384));
    
    const { result: batchResults, duration: batchDuration } = await PerformanceUtils.measureTime(async () => {
      return await client.searchBatch(collectionName, {
        searches: queryVectors.map(vector => ({
          vector,
          limit: 5,
          with_payload: ['category', 'score']
        }))
      });
    });

    console.log(`✓ Batch search (${queryVectors.length} queries) completed in ${batchDuration.toFixed(2)}ms`);
    console.log(`  Average: ${(batchDuration / queryVectors.length).toFixed(2)}ms per query`);

    // 5. Advanced query with prefetch
    console.log('\n--- Advanced Query with Prefetch ---');
    const advancedQuery = await client.query(collectionName, {
      prefetch: [
        {
          query: VectorUtils.random(384),
          filter: new FilterBuilder().equals('category', 'tech').build(),
          limit: 100
        },
        {
          query: VectorUtils.random(384),
          filter: new FilterBuilder().equals('category', 'science').build(),
          limit: 100
        }
      ],
      query: VectorUtils.random(384),
      limit: 10,
      with_payload: true
    });

    console.log(`✓ Advanced query with prefetch returned ${advancedQuery.result.length} results`);

    // 6. Scroll through large result sets
    console.log('\n--- Scrolling Through Results ---');
    let scrollOffset = undefined;
    let totalScrolled = 0;
    const scrollLimit = 50;

    while (totalScrolled < 200) {
      const scrollResponse = await client.scroll(collectionName, {
        limit: scrollLimit,
        offset: scrollOffset,
        filter: new FilterBuilder().in('category', ['tech', 'science']).build(),
        with_payload: ['category', 'index']
      });

      if (scrollResponse.result.points.length === 0) {
        break;
      }

      totalScrolled += scrollResponse.result.points.length;
      scrollOffset = scrollResponse.result.next_page_offset;

      console.log(`  Scrolled ${totalScrolled} points so far...`);

      if (!scrollOffset) {
        break;
      }
    }

    console.log(`✓ Scrolled through ${totalScrolled} points total`);

    // 7. Performance benchmarking
    console.log('\n--- Performance Benchmarking ---');
    
    const searchBenchmark = await PerformanceUtils.benchmark(
      'Single Vector Search',
      async () => {
        await client.search(collectionName, {
          vector: VectorUtils.random(384),
          limit: 10
        });
      },
      10
    );

    console.log(`✓ ${searchBenchmark.name}:`);
    console.log(`  Average: ${searchBenchmark.avgDuration.toFixed(2)}ms`);
    console.log(`  Min: ${searchBenchmark.minDuration.toFixed(2)}ms`);
    console.log(`  Max: ${searchBenchmark.maxDuration.toFixed(2)}ms`);

    // 8. Rate-limited operations
    console.log('\n--- Rate-Limited Operations ---');
    const rateLimiter = PerformanceUtils.createRateLimiter(10); // 10 requests per second
    
    const rateLimitedStart = Date.now();
    for (let i = 0; i < 5; i++) {
      await rateLimiter();
      await client.search(collectionName, {
        vector: VectorUtils.random(384),
        limit: 1
      });
    }
    const rateLimitedDuration = Date.now() - rateLimitedStart;
    
    console.log(`✓ Rate-limited 5 searches completed in ${rateLimitedDuration}ms`);
    console.log(`  Expected ~400ms for 10 RPS limit, actual: ${rateLimitedDuration}ms`);

    // 9. Snapshot operations
    console.log('\n--- Snapshot Operations ---');
    const snapshotName = await client.createSnapshot(collectionName);
    console.log(`✓ Created snapshot: ${snapshotName}`);

    const snapshots = await client.listSnapshots(collectionName);
    console.log(`✓ Collection has ${snapshots.length} snapshots`);

    // 10. Collection statistics
    console.log('\n--- Collection Statistics ---');
    const collectionInfo = await client.getCollection(collectionName);
    console.log(`✓ Collection statistics:`);
    console.log(`  Vectors: ${collectionInfo.vectors_count}`);
    console.log(`  Points: ${collectionInfo.points_count}`);
    console.log(`  Segments: ${collectionInfo.segments_count}`);
    console.log(`  Status: ${collectionInfo.status}`);

    // 11. Cleanup with filtered deletion
    console.log('\n--- Cleanup ---');
    
    // Delete points with specific criteria
    const deleteFilter = new FilterBuilder()
      .range('metadata.index', { lt: 100 })
      .build();

    const deleteResponse = await client.delete(collectionName, {
      filter: deleteFilter,
      wait: true
    });

    console.log(`✓ Deleted points with operation ID: ${deleteResponse.operation_id}`);

    // Final count
    const finalCount = await client.count(collectionName);
    console.log(`✓ Final point count: ${finalCount}`);

    // Cleanup snapshot
    await client.deleteSnapshot(collectionName, snapshotName);
    console.log(`✓ Deleted snapshot: ${snapshotName}`);

    // Delete collection
    await client.deleteCollection(collectionName);
    console.log(`✓ Deleted collection: ${collectionName}`);

    console.log('\n=== Advanced Demo Completed Successfully ===');

  } catch (error) {
    console.error('❌ Error:', error.message);
    if (error.status) {
      console.error('   Status:', error.status);
    }
    if (error.details) {
      console.error('   Details:', error.details);
    }
  } finally {
    client.close();
  }
}

// Utility function to demonstrate vector similarity
async function vectorSimilarityDemo() {
  console.log('\n=== Vector Similarity Demo ===');
  
  const vectorA = [1, 2, 3, 4, 5];
  const vectorB = [2, 3, 4, 5, 6];
  const vectorC = [-1, -2, -3, -4, -5];

  console.log('Vector A:', vectorA);
  console.log('Vector B:', vectorB);
  console.log('Vector C:', vectorC);
  console.log();

  console.log('Similarities:');
  console.log(`A ↔ B Cosine: ${VectorUtils.cosineSimilarity(vectorA, vectorB).toFixed(4)}`);
  console.log(`A ↔ C Cosine: ${VectorUtils.cosineSimilarity(vectorA, vectorC).toFixed(4)}`);
  console.log(`B ↔ C Cosine: ${VectorUtils.cosineSimilarity(vectorB, vectorC).toFixed(4)}`);
  console.log();

  console.log('Distances:');
  console.log(`A ↔ B Euclidean: ${VectorUtils.euclideanDistance(vectorA, vectorB).toFixed(4)}`);
  console.log(`A ↔ C Euclidean: ${VectorUtils.euclideanDistance(vectorA, vectorC).toFixed(4)}`);
  console.log(`B ↔ C Euclidean: ${VectorUtils.euclideanDistance(vectorB, vectorC).toFixed(4)}`);
  console.log();

  console.log('Dot Products:');
  console.log(`A · B: ${VectorUtils.dotProduct(vectorA, vectorB)}`);
  console.log(`A · C: ${VectorUtils.dotProduct(vectorA, vectorC)}`);
  console.log(`B · C: ${VectorUtils.dotProduct(vectorB, vectorC)}`);
}

// Run the examples
if (require.main === module) {
  Promise.resolve()
    .then(() => vectorSimilarityDemo())
    .then(() => advancedExample())
    .catch(console.error);
}

module.exports = { advancedExample, vectorSimilarityDemo };