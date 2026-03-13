/**
 * Utility Functions Tests
 */

import {
  VectorUtils,
  FilterBuilder,
  PointBuilder,
  BatchHelper,
  ConfigHelper,
  ValidationUtils,
  PerformanceUtils
} from '../src/utils';

describe('VectorUtils', () => {
  const vectorA = [1, 2, 3];
  const vectorB = [4, 5, 6];

  describe('cosineSimilarity', () => {
    it('should calculate cosine similarity correctly', () => {
      const similarity = VectorUtils.cosineSimilarity(vectorA, vectorB);
      expect(similarity).toBeCloseTo(0.9746, 4);
    });

    it('should handle identical vectors', () => {
      const similarity = VectorUtils.cosineSimilarity(vectorA, vectorA);
      expect(similarity).toBeCloseTo(1, 4);
    });

    it('should handle orthogonal vectors', () => {
      const orthogonal = VectorUtils.cosineSimilarity([1, 0], [0, 1]);
      expect(orthogonal).toBeCloseTo(0, 4);
    });

    it('should throw error for different dimensions', () => {
      expect(() => VectorUtils.cosineSimilarity([1, 2], [1, 2, 3])).toThrow();
    });
  });

  describe('euclideanDistance', () => {
    it('should calculate Euclidean distance correctly', () => {
      const distance = VectorUtils.euclideanDistance(vectorA, vectorB);
      expect(distance).toBeCloseTo(5.196, 3);
    });

    it('should return 0 for identical vectors', () => {
      const distance = VectorUtils.euclideanDistance(vectorA, vectorA);
      expect(distance).toBe(0);
    });
  });

  describe('dotProduct', () => {
    it('should calculate dot product correctly', () => {
      const dot = VectorUtils.dotProduct(vectorA, vectorB);
      expect(dot).toBe(32); // 1*4 + 2*5 + 3*6 = 32
    });
  });

  describe('manhattanDistance', () => {
    it('should calculate Manhattan distance correctly', () => {
      const distance = VectorUtils.manhattanDistance(vectorA, vectorB);
      expect(distance).toBe(9); // |1-4| + |2-5| + |3-6| = 3 + 3 + 3 = 9
    });
  });

  describe('normalize', () => {
    it('should normalize vector to unit length', () => {
      const normalized = VectorUtils.normalize([3, 4]);
      expect(VectorUtils.magnitude(normalized)).toBeCloseTo(1, 4);
      expect(normalized).toEqual([0.6, 0.8]);
    });

    it('should handle zero vector', () => {
      const normalized = VectorUtils.normalize([0, 0]);
      expect(normalized).toEqual([0, 0]);
    });
  });

  describe('utility functions', () => {
    it('should generate random vectors', () => {
      const random = VectorUtils.random(5, -1, 1);
      expect(random).toHaveLength(5);
      expect(random.every(val => val >= -1 && val <= 1)).toBe(true);
    });

    it('should create zero vectors', () => {
      const zeros = VectorUtils.zeros(3);
      expect(zeros).toEqual([0, 0, 0]);
    });

    it('should create ones vectors', () => {
      const ones = VectorUtils.ones(3);
      expect(ones).toEqual([1, 1, 1]);
    });

    it('should calculate magnitude', () => {
      const magnitude = VectorUtils.magnitude([3, 4]);
      expect(magnitude).toBe(5);
    });

    it('should add vectors', () => {
      const sum = VectorUtils.add([1, 2], [3, 4]);
      expect(sum).toEqual([4, 6]);
    });

    it('should subtract vectors', () => {
      const diff = VectorUtils.subtract([5, 7], [2, 3]);
      expect(diff).toEqual([3, 4]);
    });

    it('should scale vectors', () => {
      const scaled = VectorUtils.scale([1, 2, 3], 2);
      expect(scaled).toEqual([2, 4, 6]);
    });
  });
});

describe('FilterBuilder', () => {
  it('should build simple filters', () => {
    const filter = new FilterBuilder()
      .equals('category', 'test')
      .range('price', { gte: 10, lte: 100 })
      .build();

    expect(filter.must).toHaveLength(2);
    expect(filter.must![0]).toEqual({
      key: 'category',
      match: { text: 'test' }
    });
    expect(filter.must![1]).toEqual({
      key: 'price',
      range: { gte: 10, lte: 100 }
    });
  });

  it('should build complex filters with should and must_not', () => {
    const filter = new FilterBuilder()
      .should({ key: 'tag', match: { text: 'important' } })
      .should({ key: 'tag', match: { text: 'urgent' } })
      .mustNot({ key: 'status', match: { text: 'deleted' } })
      .build();

    expect(filter.should).toHaveLength(2);
    expect(filter.must_not).toHaveLength(1);
  });

  it('should handle different value types', () => {
    const filter = new FilterBuilder()
      .equals('text_field', 'string_value')
      .equals('number_field', 42)
      .equals('boolean_field', true)
      .build();

    expect(filter.must![0].match).toEqual({ text: 'string_value' });
    expect(filter.must![1].match).toEqual({ integer: 42 });
    expect(filter.must![2].match).toEqual({ boolean: true });
  });

  it('should build ID and existence filters', () => {
    const filter = new FilterBuilder()
      .hasId([1, 2, 3])
      .exists('required_field')
      .isNull('optional_field')
      .isEmpty('array_field')
      .build();

    expect(filter.must).toContainEqual({ has_id: [1, 2, 3] });
    expect(filter.must_not).toContainEqual({ is_null: { key: 'required_field' } });
    expect(filter.must).toContainEqual({ is_null: { key: 'optional_field' } });
    expect(filter.must).toContainEqual({ is_empty: { key: 'array_field' } });
  });
});

describe('PointBuilder', () => {
  it('should build valid points', () => {
    const point = new PointBuilder()
      .id('test-1')
      .vector([1, 2, 3])
      .payload({ category: 'test', score: 0.9 })
      .build();

    expect(point).toBeValidPoint();
    expect(point.id).toBe('test-1');
    expect(point.vector).toEqual([1, 2, 3]);
    expect(point.payload).toEqual({ category: 'test', score: 0.9 });
  });

  it('should add payload fields incrementally', () => {
    const point = new PointBuilder()
      .id(123)
      .vector([1, 2, 3])
      .addPayload('field1', 'value1')
      .addPayload('field2', 42)
      .build();

    expect(point.payload).toEqual({ field1: 'value1', field2: 42 });
  });

  it('should throw error for incomplete points', () => {
    expect(() => new PointBuilder().id('test').build()).toThrow();
    expect(() => new PointBuilder().vector([1, 2, 3]).build()).toThrow();
  });
});

describe('BatchHelper', () => {
  it('should chunk arrays correctly', () => {
    const array = [1, 2, 3, 4, 5, 6, 7];
    const chunks = BatchHelper.chunk(array, 3);
    
    expect(chunks).toEqual([[1, 2, 3], [4, 5, 6], [7]]);
  });

  it('should create points from vectors', () => {
    const vectors = [[1, 2], [3, 4], [5, 6]];
    const points = BatchHelper.createPoints(vectors, 10);

    expect(points).toHaveLength(3);
    expect(points[0]).toEqual({ id: 10, vector: [1, 2], payload: {} });
    expect(points[2]).toEqual({ id: 12, vector: [5, 6], payload: {} });
  });

  it('should extract vectors from points', () => {
    const points = [
      { id: 1, vector: [1, 2], payload: {} },
      { id: 2, vector: [3, 4], payload: {} }
    ];
    const vectors = BatchHelper.extractVectors(points);

    expect(vectors).toEqual([[1, 2], [3, 4]]);
  });

  it('should extract payloads from points', () => {
    const points = [
      { id: 1, vector: [1, 2], payload: { a: 1 } },
      { id: 2, vector: [3, 4], payload: { b: 2 } }
    ];
    const payloads = BatchHelper.extractPayloads(points);

    expect(payloads).toEqual([{ a: 1 }, { b: 2 }]);
  });
});

describe('ConfigHelper', () => {
  it('should create default HNSW config', () => {
    const config = ConfigHelper.defaultHnsw();
    expect(config.m).toBe(16);
    expect(config.ef_construct).toBe(100);
  });

  it('should create optimized configs', () => {
    const fast = ConfigHelper.fastHnsw();
    const accurate = ConfigHelper.accurateHnsw();

    expect(fast.m).toBeLessThan(accurate.m);
    expect(fast.ef_construct).toBeLessThan(accurate.ef_construct);
  });

  it('should create quantization configs', () => {
    const scalar = ConfigHelper.scalarQuantization(0.95);
    const product = ConfigHelper.productQuantization('x8');
    const binary = ConfigHelper.binaryQuantization();

    expect(scalar.scalar.quantile).toBe(0.95);
    expect(product.product.compression).toBe('x8');
    expect(binary.binary).toBeDefined();
  });
});

describe('ValidationUtils', () => {
  it('should validate vectors', () => {
    expect(ValidationUtils.validateVector([1, 2, 3])).toBe(true);
    expect(ValidationUtils.validateVector([1, 2, 3], 3)).toBe(true);
    expect(ValidationUtils.validateVector([1, 2, 3], 4)).toBe(false);
    expect(ValidationUtils.validateVector([])).toBe(false);
    expect(ValidationUtils.validateVector([1, 'invalid', 3])).toBe(false);
    expect(ValidationUtils.validateVector('not-array')).toBe(false);
  });

  it('should validate collection names', () => {
    expect(ValidationUtils.validateCollectionName('valid_name')).toBe(true);
    expect(ValidationUtils.validateCollectionName('valid-name')).toBe(true);
    expect(ValidationUtils.validateCollectionName('valid123')).toBe(true);
    expect(ValidationUtils.validateCollectionName('')).toBe(false);
    expect(ValidationUtils.validateCollectionName('invalid@name')).toBe(false);
    expect(ValidationUtils.validateCollectionName('a'.repeat(256))).toBe(false);
  });

  it('should validate point IDs', () => {
    expect(ValidationUtils.validatePointId('string-id')).toBe(true);
    expect(ValidationUtils.validatePointId(123)).toBe(true);
    expect(ValidationUtils.validatePointId(null)).toBe(false);
    expect(ValidationUtils.validatePointId({})).toBe(false);
  });

  it('should validate distance metrics', () => {
    expect(ValidationUtils.validateDistanceMetric('Cosine')).toBe(true);
    expect(ValidationUtils.validateDistanceMetric('Euclidean')).toBe(true);
    expect(ValidationUtils.validateDistanceMetric('Invalid')).toBe(false);
  });
});

describe('PerformanceUtils', () => {
  it('should measure execution time', async () => {
    const testFn = async (): Promise<string> => {
      await new Promise(resolve => setTimeout(resolve, 10));
      return 'result';
    };

    const { result, duration } = await PerformanceUtils.measureTime(testFn);
    expect(result).toBe('result');
    expect(duration).toBeGreaterThan(5);
  });

  it('should run benchmarks', async () => {
    const testFn = async (): Promise<void> => {
      await new Promise(resolve => setTimeout(resolve, 1));
    };

    const benchmark = await PerformanceUtils.benchmark('test', testFn, 3);
    expect(benchmark.name).toBe('test');
    expect(benchmark.avgDuration).toBeGreaterThan(0);
    expect(benchmark.minDuration).toBeLessThanOrEqual(benchmark.avgDuration);
    expect(benchmark.maxDuration).toBeGreaterThanOrEqual(benchmark.avgDuration);
  });

  it('should create rate limiter', async () => {
    const rateLimiter = PerformanceUtils.createRateLimiter(10); // 10 requests per second
    
    const start = Date.now();
    await rateLimiter();
    await rateLimiter();
    const duration = Date.now() - start;

    expect(duration).toBeGreaterThan(90); // Should take at least ~100ms for 2 requests at 10 RPS
  });
});