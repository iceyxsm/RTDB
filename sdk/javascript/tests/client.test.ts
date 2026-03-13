/**
 * RTDB Client Tests
 */

import { RTDBClient } from '../src/client';
import { ValidationError, ConnectionError } from '../src/types';

// Mock HTTP client
jest.mock('../src/http-client');

describe('RTDBClient', () => {
  let client: RTDBClient;
  const mockConfig = {
    url: 'http://localhost:6333',
    apiKey: 'test-key'
  };

  beforeEach(() => {
    client = new RTDBClient(mockConfig);
  });

  afterEach(() => {
    client.close();
  });

  describe('Configuration', () => {
    it('should validate required URL', () => {
      expect(() => new RTDBClient({ url: '' })).toThrow(ValidationError);
    });

    it('should add protocol to URL if missing', () => {
      const client = new RTDBClient({ url: 'localhost:6333' });
      expect(client.getConfig().url).toBe('http://localhost:6333');
    });

    it('should remove trailing slash from URL', () => {
      const client = new RTDBClient({ url: 'http://localhost:6333/' });
      expect(client.getConfig().url).toBe('http://localhost:6333');
    });

    it('should add port to URL if specified', () => {
      const client = new RTDBClient({ url: 'http://localhost', port: 6333 });
      expect(client.getConfig().url).toBe('http://localhost:6333');
    });

    it('should set default configuration values', () => {
      const config = client.getConfig();
      expect(config.timeout).toBe(30000);
      expect(config.retries).toBe(3);
      expect(config.retryDelay).toBe(100);
    });
  });

  describe('Collection Name Validation', () => {
    it('should reject empty collection names', async () => {
      await expect(client.getCollection('')).rejects.toThrow(ValidationError);
    });

    it('should reject non-string collection names', async () => {
      await expect(client.getCollection(123 as any)).rejects.toThrow(ValidationError);
    });

    it('should reject collection names with invalid characters', async () => {
      await expect(client.getCollection('test@collection')).rejects.toThrow(ValidationError);
    });

    it('should reject collection names that are too long', async () => {
      const longName = 'a'.repeat(256);
      await expect(client.getCollection(longName)).rejects.toThrow(ValidationError);
    });

    it('should accept valid collection names', async () => {
      const validNames = ['test', 'test_collection', 'test-collection', 'test123'];
      
      for (const name of validNames) {
        // Should not throw
        expect(() => (client as any).validateCollectionName(name)).not.toThrow();
      }
    });
  });

  describe('Collection Configuration Validation', () => {
    it('should reject invalid vector_size', () => {
      const invalidConfigs = [
        { vector_size: 0, distance: 'Cosine' },
        { vector_size: -1, distance: 'Cosine' },
        { vector_size: 65537, distance: 'Cosine' }
      ];

      for (const config of invalidConfigs) {
        expect(() => (client as any).validateCollectionConfig(config)).toThrow(ValidationError);
      }
    });

    it('should reject invalid distance metrics', () => {
      const config = { vector_size: 128, distance: 'Invalid' };
      expect(() => (client as any).validateCollectionConfig(config)).toThrow(ValidationError);
    });

    it('should accept valid collection configurations', () => {
      const validConfigs = [
        { vector_size: 128, distance: 'Cosine' },
        { vector_size: 768, distance: 'Euclidean' },
        { vector_size: 1536, distance: 'Dot' },
        { vector_size: 512, distance: 'Manhattan' }
      ];

      for (const config of validConfigs) {
        expect(() => (client as any).validateCollectionConfig(config)).not.toThrow();
      }
    });
  });

  describe('Upsert Request Validation', () => {
    it('should reject empty points array', () => {
      const request = { points: [] };
      expect(() => (client as any).validateUpsertRequest(request)).toThrow(ValidationError);
    });

    it('should reject too many points', () => {
      const points = Array(10001).fill({ id: 1, vector: [1, 2, 3] });
      const request = { points };
      expect(() => (client as any).validateUpsertRequest(request)).toThrow(ValidationError);
    });

    it('should reject points without vectors', () => {
      const request = { points: [{ id: 1 }] };
      expect(() => (client as any).validateUpsertRequest(request)).toThrow(ValidationError);
    });

    it('should reject points without IDs', () => {
      const request = { points: [{ vector: [1, 2, 3] }] };
      expect(() => (client as any).validateUpsertRequest(request)).toThrow(ValidationError);
    });

    it('should accept valid upsert requests', () => {
      const request = {
        points: [
          { id: 1, vector: [1, 2, 3] },
          { id: 2, vector: [4, 5, 6], payload: { category: 'test' } }
        ]
      };
      expect(() => (client as any).validateUpsertRequest(request)).not.toThrow();
    });
  });

  describe('Search Request Validation', () => {
    it('should reject search without vector', () => {
      const request = { limit: 10 };
      expect(() => (client as any).validateSearchRequest(request)).toThrow(ValidationError);
    });

    it('should reject invalid limit values', () => {
      const invalidRequests = [
        { vector: [1, 2, 3], limit: 0 },
        { vector: [1, 2, 3], limit: -1 },
        { vector: [1, 2, 3], limit: 10001 }
      ];

      for (const request of invalidRequests) {
        expect(() => (client as any).validateSearchRequest(request)).toThrow(ValidationError);
      }
    });

    it('should reject negative offset', () => {
      const request = { vector: [1, 2, 3], offset: -1 };
      expect(() => (client as any).validateSearchRequest(request)).toThrow(ValidationError);
    });

    it('should accept valid search requests', () => {
      const validRequests = [
        { vector: [1, 2, 3] },
        { vector: [1, 2, 3], limit: 10 },
        { vector: [1, 2, 3], limit: 10, offset: 5 },
        { vector: [1, 2, 3], score_threshold: 0.8 }
      ];

      for (const request of validRequests) {
        expect(() => (client as any).validateSearchRequest(request)).not.toThrow();
      }
    });
  });

  describe('Batch Search Request Validation', () => {
    it('should reject empty searches array', () => {
      const request = { searches: [] };
      expect(() => (client as any).validateBatchSearchRequest(request)).toThrow(ValidationError);
    });

    it('should reject too many searches', () => {
      const searches = Array(101).fill({ vector: [1, 2, 3] });
      const request = { searches };
      expect(() => (client as any).validateBatchSearchRequest(request)).toThrow(ValidationError);
    });

    it('should validate individual search requests', () => {
      const request = {
        searches: [
          { vector: [1, 2, 3] },
          { limit: 10 } // Invalid - no vector
        ]
      };
      expect(() => (client as any).validateBatchSearchRequest(request)).toThrow(ValidationError);
    });

    it('should accept valid batch search requests', () => {
      const request = {
        searches: [
          { vector: [1, 2, 3], limit: 10 },
          { vector: [4, 5, 6], limit: 5 }
        ]
      };
      expect(() => (client as any).validateBatchSearchRequest(request)).not.toThrow();
    });
  });

  describe('Retrieve Request Validation', () => {
    it('should reject empty IDs array', () => {
      const request = { ids: [] };
      expect(() => (client as any).validateRetrieveRequest(request)).toThrow(ValidationError);
    });

    it('should reject too many IDs', () => {
      const ids = Array(10001).fill(1);
      const request = { ids };
      expect(() => (client as any).validateRetrieveRequest(request)).toThrow(ValidationError);
    });

    it('should accept valid retrieve requests', () => {
      const request = { ids: [1, 2, 3, 'test'] };
      expect(() => (client as any).validateRetrieveRequest(request)).not.toThrow();
    });
  });

  describe('Delete Request Validation', () => {
    it('should reject delete without points or filter', () => {
      const request = {};
      expect(() => (client as any).validateDeleteRequest(request)).toThrow(ValidationError);
    });

    it('should reject too many points to delete', () => {
      const points = Array(10001).fill(1);
      const request = { points };
      expect(() => (client as any).validateDeleteRequest(request)).toThrow(ValidationError);
    });

    it('should accept valid delete requests', () => {
      const validRequests = [
        { points: [1, 2, 3] },
        { filter: { must: [{ key: 'category', match: { text: 'test' } }] } },
        { points: [1], filter: { must: [] } }
      ];

      for (const request of validRequests) {
        expect(() => (client as any).validateDeleteRequest(request)).not.toThrow();
      }
    });
  });

  describe('Client Methods', () => {
    it('should clone client with overrides', () => {
      const cloned = client.clone({ timeout: 60000 });
      expect(cloned.getConfig().timeout).toBe(60000);
      expect(cloned.getConfig().url).toBe(mockConfig.url);
    });

    it('should close client properly', () => {
      const destroySpy = jest.spyOn(client['http'], 'destroy');
      client.close();
      expect(destroySpy).toHaveBeenCalled();
    });
  });
});