/**
 * Integration Tests for RTDB Client
 * These tests require a running RTDB server
 */

import { RTDBClient } from '../src/client';
import { FilterBuilder, PointBuilder, VectorUtils } from '../src/utils';

// Skip integration tests if RTDB server is not available
const RTDB_URL = process.env.RTDB_URL || 'http://localhost:6333';
const RTDB_API_KEY = process.env.RTDB_API_KEY;
const RUN_INTEGRATION_TESTS = process.env.RUN_INTEGRATION_TESTS === 'true';

const describeIntegration = RUN_INTEGRATION_TESTS ? describe : describe.skip;

describeIntegration('RTDB Integration Tests', () => {
  let client: RTDBClient;
  const testCollectionName = `test_collection_${Date.now()}`;

  beforeAll(async () => {
    client = new RTDBClient({
      url: RTDB_URL,
      apiKey: RTDB_API_KEY,
      timeout: 10000
    });

    // Wait for server to be ready
    let retries = 10;
    while (retries > 0) {
      try {
        await client.isReady();
        break;
      } catch (error) {
        retries--;
        if (retries === 0) {
          throw new Error('RTDB server is not ready');
        }
        await new Promise(resolve => setTimeout(resolve, 1000));
      }
    }
  });

  afterAll(async () => {
    // Cleanup test collection
    try {
      await client.deleteCollection(testCollectionName);
    } catch (error) {
      // Ignore cleanup errors
    }
    client.close();
  });

  describe('Service Health', () => {
    it('should check service health', async () => {
      const health = await client.healthCheck();
      expect(health.title).toContain('rtdb');
      expect(health.version).toBeDefined();
    });

    it('should check readiness', async () => {
      const isReady = await client.isReady();
      expect(isReady).toBe(true);
    });

    it('should check liveness', async () => {
      const isAlive = await client.isAlive();
      expect(isAlive).toBe(true);
    });
  });

  describe('Collection Management', () => {
    it('should create a collection', async () => {
      const result = await client.createCollection(testCollectionName, {
        vector_size: 128,
        distance: 'Cosine'
      });
      expect(result).toBe(true);
    });

    it('should list collections', async () => {
      const collections = await client.listCollections();
      expect(collections).toContain(testCollectionName);
    });

    it('should get collection info', async () => {
      const info = await client.getCollection(testCollectionName);
      expect(info.config.vector_size).toBe(128);
      expect(info.config.distance).toBe('Cosine');
    });

    it('should check collection existence', async () => {
      const exists = await client.collectionExists(testCollectionName);
      expect(exists).toBe(true);

      const notExists = await client.collectionExists('non_existent_collection');
      expect(notExists).toBe(false);
    });
  });

  describe('Point Operations', () => {
    const testPoints = [
      new PointBuilder()
        .id('point-1')
        .vector(VectorUtils.random(128))
        .addPayload('category', 'test')
        .addPayload('score', 0.9)
        .build(),
      new PointBuilder()
        .id('point-2')
        .vector(VectorUtils.random(128))
        .addPayload('category', 'test')
        .addPayload('score', 0.8)
        .build(),
      new PointBuilder()
        .id('point-3')
        .vector(VectorUtils.random(128))
        .addPayload('category', 'demo')
        .addPayload('score', 0.7)
        .build()
    ];

    it('should upsert points', async () => {
      const response = await client.upsert(testCollectionName, {
        points: testPoints
      });
      expect(response.operation_id).toBeDefined();
      expect(response.status).toBeDefined();

      // Wait for operation to complete
      await new Promise(resolve => setTimeout(resolve, 1000));
    });

    it('should retrieve points by ID', async () => {
      const point = await client.getPoint(testCollectionName, 'point-1');
      expect(point).not.toBeNull();
      expect(point!.id).toBe('point-1');
      expect(point!.payload?.category).toBe('test');
    });

    it('should retrieve multiple points', async () => {
      const response = await client.retrieve(testCollectionName, {
        ids: ['point-1', 'point-2'],
        with_payload: true,
        with_vector: false
      });

      expect(response.result).toHaveLength(2);
      expect(response.result[0].payload).toBeDefined();
      expect(response.result[0].vector).toBeUndefined();
    });

    it('should search for similar vectors', async () => {
      const queryVector = testPoints[0].vector;
      const response = await client.search(testCollectionName, {
        vector: queryVector,
        limit: 2,
        with_payload: true
      });

      expect(response.result).toHaveLength(2);
      expect(response.result[0].score).toBeGreaterThan(0);
      expect(response.result[0].payload).toBeDefined();
    });

    it('should search with filters', async () => {
      const filter = new FilterBuilder()
        .equals('category', 'test')
        .range('score', { gte: 0.8 })
        .build();

      const response = await client.search(testCollectionName, {
        vector: testPoints[0].vector,
        limit: 10,
        filter,
        with_payload: true
      });

      expect(response.result.length).toBeGreaterThan(0);
      response.result.forEach(result => {
        expect(result.payload?.category).toBe('test');
        expect(result.payload?.score).toBeGreaterThanOrEqual(0.8);
      });
    });

    it('should perform batch search', async () => {
      const response = await client.searchBatch(testCollectionName, {
        searches: [
          { vector: testPoints[0].vector, limit: 1 },
          { vector: testPoints[1].vector, limit: 1 }
        ]
      });

      expect(response.result).toHaveLength(2);
      expect(response.result[0]).toHaveLength(1);
      expect(response.result[1]).toHaveLength(1);
    });

    it('should scroll through points', async () => {
      const response = await client.scroll(testCollectionName, {
        limit: 2,
        with_payload: true
      });

      expect(response.result.points.length).toBeGreaterThan(0);
      expect(response.result.points.length).toBeLessThanOrEqual(2);
    });

    it('should count points', async () => {
      const count = await client.count(testCollectionName);
      expect(count).toBeGreaterThanOrEqual(3);
    });

    it('should count points with filter', async () => {
      const filter = new FilterBuilder()
        .equals('category', 'test')
        .build();

      const count = await client.count(testCollectionName, { filter });
      expect(count).toBe(2);
    });

    it('should delete points by ID', async () => {
      const result = await client.deletePoint(testCollectionName, 'point-3');
      expect(result).toBe(true);

      // Verify deletion
      const point = await client.getPoint(testCollectionName, 'point-3');
      expect(point).toBeNull();
    });

    it('should delete points with filter', async () => {
      const filter = new FilterBuilder()
        .equals('category', 'test')
        .build();

      const response = await client.delete(testCollectionName, {
        filter,
        wait: true
      });

      expect(response.operation_id).toBeDefined();

      // Wait for operation to complete
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Verify deletion
      const count = await client.count(testCollectionName, { filter });
      expect(count).toBe(0);
    });
  });

  describe('Advanced Features', () => {
    beforeEach(async () => {
      // Add some test data
      const points = Array.from({ length: 10 }, (_, i) => 
        new PointBuilder()
          .id(`advanced-${i}`)
          .vector(VectorUtils.random(128))
          .addPayload('index', i)
          .addPayload('category', i % 2 === 0 ? 'even' : 'odd')
          .build()
      );

      await client.upsert(testCollectionName, { points });
      await new Promise(resolve => setTimeout(resolve, 1000));
    });

    it('should perform query with prefetch', async () => {
      const response = await client.query(testCollectionName, {
        prefetch: [{
          query: VectorUtils.random(128),
          limit: 5
        }],
        query: VectorUtils.random(128),
        limit: 3,
        with_payload: true
      });

      expect(response.result.length).toBeLessThanOrEqual(3);
    });

    it('should handle search with score threshold', async () => {
      const response = await client.search(testCollectionName, {
        vector: VectorUtils.random(128),
        limit: 10,
        score_threshold: 0.5,
        with_payload: true
      });

      response.result.forEach(result => {
        expect(result.score).toBeGreaterThanOrEqual(0.5);
      });
    });

    it('should handle complex filters', async () => {
      const filter = new FilterBuilder()
        .should(new FilterBuilder().equals('category', 'even').build())
        .should(new FilterBuilder().range('index', { gte: 8 }).build())
        .mustNot(new FilterBuilder().equals('index', 0).build())
        .build();

      const response = await client.search(testCollectionName, {
        vector: VectorUtils.random(128),
        limit: 10,
        filter,
        with_payload: true
      });

      response.result.forEach(result => {
        const index = result.payload?.index;
        const category = result.payload?.category;
        expect(
          (category === 'even' || index >= 8) && index !== 0
        ).toBe(true);
      });
    });
  });

  describe('Snapshot Operations', () => {
    it('should create and list snapshots', async () => {
      const snapshotName = await client.createSnapshot(testCollectionName);
      expect(snapshotName).toBeDefined();

      const snapshots = await client.listSnapshots(testCollectionName);
      expect(snapshots).toContain(snapshotName);

      // Cleanup
      await client.deleteSnapshot(testCollectionName, snapshotName);
    });

    it('should create and list full snapshots', async () => {
      const snapshotName = await client.createFullSnapshot();
      expect(snapshotName).toBeDefined();

      const snapshots = await client.listFullSnapshots();
      expect(snapshots).toContain(snapshotName);
    });
  });

  describe('Error Handling', () => {
    it('should handle collection not found', async () => {
      await expect(
        client.getCollection('non_existent_collection')
      ).rejects.toThrow();
    });

    it('should handle invalid vector dimensions', async () => {
      const invalidPoint = new PointBuilder()
        .id('invalid')
        .vector([1, 2]) // Wrong dimension
        .build();

      await expect(
        client.upsert(testCollectionName, { points: [invalidPoint] })
      ).rejects.toThrow();
    });

    it('should handle search with invalid vector', async () => {
      await expect(
        client.search(testCollectionName, {
          vector: [1, 2] // Wrong dimension
        })
      ).rejects.toThrow();
    });
  });

  describe('Performance', () => {
    it('should handle concurrent requests', async () => {
      const promises = Array.from({ length: 10 }, (_, i) =>
        client.search(testCollectionName, {
          vector: VectorUtils.random(128),
          limit: 5
        })
      );

      const results = await Promise.all(promises);
      expect(results).toHaveLength(10);
      results.forEach(result => {
        expect(result.result.length).toBeLessThanOrEqual(5);
      });
    });

    it('should handle large batch operations', async () => {
      const largePoints = Array.from({ length: 1000 }, (_, i) =>
        new PointBuilder()
          .id(`large-${i}`)
          .vector(VectorUtils.random(128))
          .addPayload('batch', 'large')
          .build()
      );

      const response = await client.upsert(testCollectionName, {
        points: largePoints
      });

      expect(response.operation_id).toBeDefined();

      // Wait for operation to complete
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Verify count
      const filter = new FilterBuilder().equals('batch', 'large').build();
      const count = await client.count(testCollectionName, { filter });
      expect(count).toBe(1000);

      // Cleanup
      await client.delete(testCollectionName, { filter });
    });
  });
});