/**
 * Basic Usage Example for RTDB JavaScript SDK
 */

const { RTDBClient, FilterBuilder, PointBuilder, VectorUtils } = require('@rtdb/client');

async function basicExample() {
  // Create client
  const client = new RTDBClient({
    url: 'http://localhost:6333',
    apiKey: 'your-api-key' // Optional
  });

  try {
    // Check if server is ready
    const isReady = await client.isReady();
    console.log('Server ready:', isReady);

    // Create a collection
    const collectionName = 'my_collection';
    await client.createCollection(collectionName, {
      vector_size: 128,
      distance: 'Cosine'
    });
    console.log('Collection created');

    // Create some points
    const points = [
      new PointBuilder()
        .id('doc-1')
        .vector(VectorUtils.random(128))
        .addPayload('title', 'First document')
        .addPayload('category', 'tech')
        .build(),
      
      new PointBuilder()
        .id('doc-2')
        .vector(VectorUtils.random(128))
        .addPayload('title', 'Second document')
        .addPayload('category', 'science')
        .build(),
      
      new PointBuilder()
        .id('doc-3')
        .vector(VectorUtils.random(128))
        .addPayload('title', 'Third document')
        .addPayload('category', 'tech')
        .build()
    ];

    // Insert points
    const upsertResponse = await client.upsert(collectionName, { points });
    console.log('Points inserted:', upsertResponse.operation_id);

    // Wait a moment for indexing
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Search for similar vectors
    const searchResponse = await client.search(collectionName, {
      vector: points[0].vector, // Use first point's vector as query
      limit: 2,
      with_payload: true
    });

    console.log('Search results:');
    searchResponse.result.forEach((result, index) => {
      console.log(`  ${index + 1}. ID: ${result.id}, Score: ${result.score.toFixed(4)}`);
      console.log(`     Title: ${result.payload?.title}`);
      console.log(`     Category: ${result.payload?.category}`);
    });

    // Search with filter
    const filter = new FilterBuilder()
      .equals('category', 'tech')
      .build();

    const filteredSearch = await client.search(collectionName, {
      vector: VectorUtils.random(128),
      limit: 10,
      filter,
      with_payload: true
    });

    console.log('\nFiltered search results (tech category):');
    filteredSearch.result.forEach((result, index) => {
      console.log(`  ${index + 1}. ID: ${result.id}, Title: ${result.payload?.title}`);
    });

    // Get a specific point
    const point = await client.getPoint(collectionName, 'doc-1', true, false);
    console.log('\nRetrieved point:', {
      id: point?.id,
      title: point?.payload?.title,
      hasVector: !!point?.vector
    });

    // Count points
    const totalCount = await client.count(collectionName);
    console.log('\nTotal points:', totalCount);

    const techCount = await client.count(collectionName, { filter });
    console.log('Tech category points:', techCount);

    // List collections
    const collections = await client.listCollections();
    console.log('\nAll collections:', collections);

    // Cleanup
    await client.deleteCollection(collectionName);
    console.log('Collection deleted');

  } catch (error) {
    console.error('Error:', error.message);
    if (error.status) {
      console.error('Status:', error.status);
    }
  } finally {
    // Always close the client
    client.close();
  }
}

// Run the example
if (require.main === module) {
  basicExample().catch(console.error);
}

module.exports = { basicExample };