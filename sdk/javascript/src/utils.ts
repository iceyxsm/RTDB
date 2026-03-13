/**
 * Utility functions for RTDB JavaScript SDK
 */

import { Vector, Point, Filter, DistanceMetric } from './types';

/**
 * Calculate vector similarity using different distance metrics
 */
export class VectorUtils {
  /**
   * Calculate cosine similarity between two vectors
   */
  static cosineSimilarity(a: Vector, b: Vector): number {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const magnitude = Math.sqrt(normA) * Math.sqrt(normB);
    return magnitude === 0 ? 0 : dotProduct / magnitude;
  }

  /**
   * Calculate Euclidean distance between two vectors
   */
  static euclideanDistance(a: Vector, b: Vector): number {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }

    let sum = 0;
    for (let i = 0; i < a.length; i++) {
      const diff = a[i] - b[i];
      sum += diff * diff;
    }

    return Math.sqrt(sum);
  }

  /**
   * Calculate dot product between two vectors
   */
  static dotProduct(a: Vector, b: Vector): number {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }

    let sum = 0;
    for (let i = 0; i < a.length; i++) {
      sum += a[i] * b[i];
    }

    return sum;
  }

  /**
   * Calculate Manhattan distance between two vectors
   */
  static manhattanDistance(a: Vector, b: Vector): number {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }

    let sum = 0;
    for (let i = 0; i < a.length; i++) {
      sum += Math.abs(a[i] - b[i]);
    }

    return sum;
  }

  /**
   * Normalize a vector to unit length
   */
  static normalize(vector: Vector): Vector {
    const magnitude = Math.sqrt(vector.reduce((sum, val) => sum + val * val, 0));
    return magnitude === 0 ? vector : vector.map(val => val / magnitude);
  }

  /**
   * Generate a random vector of specified dimension
   */
  static random(dimension: number, min: number = -1, max: number = 1): Vector {
    const vector: Vector = [];
    for (let i = 0; i < dimension; i++) {
      vector.push(Math.random() * (max - min) + min);
    }
    return vector;
  }

  /**
   * Create a zero vector of specified dimension
   */
  static zeros(dimension: number): Vector {
    return new Array(dimension).fill(0);
  }

  /**
   * Create a ones vector of specified dimension
   */
  static ones(dimension: number): Vector {
    return new Array(dimension).fill(1);
  }

  /**
   * Calculate vector magnitude (L2 norm)
   */
  static magnitude(vector: Vector): number {
    return Math.sqrt(vector.reduce((sum, val) => sum + val * val, 0));
  }

  /**
   * Add two vectors element-wise
   */
  static add(a: Vector, b: Vector): Vector {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }
    return a.map((val, i) => val + b[i]);
  }

  /**
   * Subtract two vectors element-wise
   */
  static subtract(a: Vector, b: Vector): Vector {
    if (a.length !== b.length) {
      throw new Error('Vectors must have the same dimension');
    }
    return a.map((val, i) => val - b[i]);
  }

  /**
   * Multiply vector by scalar
   */
  static scale(vector: Vector, scalar: number): Vector {
    return vector.map(val => val * scalar);
  }
}

/**
 * Filter builder for creating complex queries
 */
export class FilterBuilder {
  private filter: Filter = {};

  /**
   * Add a must condition (AND logic)
   */
  must(condition: any): FilterBuilder {
    if (!this.filter.must) {
      this.filter.must = [];
    }
    this.filter.must.push(condition);
    return this;
  }

  /**
   * Add a should condition (OR logic)
   */
  should(condition: any): FilterBuilder {
    if (!this.filter.should) {
      this.filter.should = [];
    }
    this.filter.should.push(condition);
    return this;
  }

  /**
   * Add a must_not condition (NOT logic)
   */
  mustNot(condition: any): FilterBuilder {
    if (!this.filter.must_not) {
      this.filter.must_not = [];
    }
    this.filter.must_not.push(condition);
    return this;
  }

  /**
   * Add field equals condition
   */
  equals(field: string, value: any): FilterBuilder {
    return this.must({
      key: field,
      match: this.createMatchValue(value)
    });
  }

  /**
   * Add field range condition
   */
  range(field: string, options: { gt?: number; gte?: number; lt?: number; lte?: number }): FilterBuilder {
    return this.must({
      key: field,
      range: options
    });
  }

  /**
   * Add field in array condition
   */
  in(field: string, values: any[]): FilterBuilder {
    return this.must({
      key: field,
      match: { any: values }
    });
  }

  /**
   * Add has ID condition
   */
  hasId(ids: (string | number)[]): FilterBuilder {
    return this.must({
      has_id: ids
    });
  }

  /**
   * Add field exists condition
   */
  exists(field: string): FilterBuilder {
    return this.mustNot({
      is_null: { key: field }
    });
  }

  /**
   * Add field is null condition
   */
  isNull(field: string): FilterBuilder {
    return this.must({
      is_null: { key: field }
    });
  }

  /**
   * Add field is empty condition
   */
  isEmpty(field: string): FilterBuilder {
    return this.must({
      is_empty: { key: field }
    });
  }

  /**
   * Build the final filter object
   */
  build(): Filter {
    return this.filter;
  }

  /**
   * Create appropriate match value based on type
   */
  private createMatchValue(value: any): any {
    if (typeof value === 'string') {
      return { text: value };
    } else if (typeof value === 'number') {
      return { integer: value };
    } else if (typeof value === 'boolean') {
      return { boolean: value };
    } else {
      return { keyword: String(value) };
    }
  }
}

/**
 * Point builder for creating points with fluent API
 */
export class PointBuilder {
  private point: Partial<Point> = {};

  /**
   * Set point ID
   */
  id(id: string | number): PointBuilder {
    this.point.id = id;
    return this;
  }

  /**
   * Set point vector
   */
  vector(vector: Vector): PointBuilder {
    this.point.vector = vector;
    return this;
  }

  /**
   * Set point payload
   */
  payload(payload: Record<string, any>): PointBuilder {
    this.point.payload = payload;
    return this;
  }

  /**
   * Add payload field
   */
  addPayload(key: string, value: any): PointBuilder {
    if (!this.point.payload) {
      this.point.payload = {};
    }
    this.point.payload[key] = value;
    return this;
  }

  /**
   * Build the final point object
   */
  build(): Point {
    if (!this.point.id || !this.point.vector) {
      throw new Error('Point must have both id and vector');
    }
    return this.point as Point;
  }
}

/**
 * Batch operations helper
 */
export class BatchHelper {
  /**
   * Split array into chunks of specified size
   */
  static chunk<T>(array: T[], size: number): T[][] {
    const chunks: T[][] = [];
    for (let i = 0; i < array.length; i += size) {
      chunks.push(array.slice(i, i + size));
    }
    return chunks;
  }

  /**
   * Create points from vectors with auto-generated IDs
   */
  static createPoints(vectors: Vector[], startId: number = 0): Point[] {
    return vectors.map((vector, index) => ({
      id: startId + index,
      vector,
      payload: {}
    }));
  }

  /**
   * Extract vectors from points
   */
  static extractVectors(points: Point[]): Vector[] {
    return points.map(point => point.vector);
  }

  /**
   * Extract payloads from points
   */
  static extractPayloads(points: Point[]): Record<string, any>[] {
    return points.map(point => point.payload || {});
  }
}

/**
 * Configuration helpers
 */
export class ConfigHelper {
  /**
   * Create default HNSW configuration
   */
  static defaultHnsw() {
    return {
      m: 16,
      ef_construct: 100,
      full_scan_threshold: 10000,
      max_indexing_threads: 0,
      on_disk: false
    };
  }

  /**
   * Create optimized HNSW configuration for speed
   */
  static fastHnsw() {
    return {
      m: 8,
      ef_construct: 64,
      full_scan_threshold: 20000,
      max_indexing_threads: 0,
      on_disk: false
    };
  }

  /**
   * Create optimized HNSW configuration for accuracy
   */
  static accurateHnsw() {
    return {
      m: 32,
      ef_construct: 200,
      full_scan_threshold: 5000,
      max_indexing_threads: 0,
      on_disk: false
    };
  }

  /**
   * Create scalar quantization configuration
   */
  static scalarQuantization(quantile: number = 0.99) {
    return {
      scalar: {
        type: 'int8' as const,
        quantile,
        always_ram: false
      }
    };
  }

  /**
   * Create product quantization configuration
   */
  static productQuantization(compression: 'x4' | 'x8' | 'x16' | 'x32' | 'x64' = 'x16') {
    return {
      product: {
        compression,
        always_ram: false
      }
    };
  }

  /**
   * Create binary quantization configuration
   */
  static binaryQuantization() {
    return {
      binary: {
        always_ram: false
      }
    };
  }
}

/**
 * Validation utilities
 */
export class ValidationUtils {
  /**
   * Validate vector dimension
   */
  static validateVector(vector: Vector, expectedDimension?: number): boolean {
    if (!Array.isArray(vector)) {
      return false;
    }
    if (vector.length === 0) {
      return false;
    }
    if (expectedDimension && vector.length !== expectedDimension) {
      return false;
    }
    return vector.every(val => typeof val === 'number' && !isNaN(val));
  }

  /**
   * Validate collection name
   */
  static validateCollectionName(name: string): boolean {
    if (typeof name !== 'string' || name.length === 0) {
      return false;
    }
    if (name.length > 255) {
      return false;
    }
    return /^[a-zA-Z0-9_-]+$/.test(name);
  }

  /**
   * Validate point ID
   */
  static validatePointId(id: any): boolean {
    return typeof id === 'string' || typeof id === 'number';
  }

  /**
   * Validate distance metric
   */
  static validateDistanceMetric(metric: string): metric is DistanceMetric {
    return ['Cosine', 'Euclidean', 'Dot', 'Manhattan'].includes(metric);
  }
}

/**
 * Performance utilities
 */
export class PerformanceUtils {
  /**
   * Measure execution time of a function
   */
  static async measureTime<T>(fn: () => Promise<T>): Promise<{ result: T; duration: number }> {
    const start = performance.now();
    const result = await fn();
    const duration = performance.now() - start;
    return { result, duration };
  }

  /**
   * Create a simple benchmark for operations
   */
  static async benchmark<T>(
    name: string,
    fn: () => Promise<T>,
    iterations: number = 1
  ): Promise<{ name: string; avgDuration: number; minDuration: number; maxDuration: number }> {
    const durations: number[] = [];
    
    for (let i = 0; i < iterations; i++) {
      const { duration } = await this.measureTime(fn);
      durations.push(duration);
    }

    return {
      name,
      avgDuration: durations.reduce((sum, d) => sum + d, 0) / durations.length,
      minDuration: Math.min(...durations),
      maxDuration: Math.max(...durations)
    };
  }

  /**
   * Create a rate limiter
   */
  static createRateLimiter(requestsPerSecond: number) {
    let lastRequest = 0;
    const interval = 1000 / requestsPerSecond;

    return async (): Promise<void> => {
      const now = Date.now();
      const timeSinceLastRequest = now - lastRequest;
      
      if (timeSinceLastRequest < interval) {
        await new Promise(resolve => setTimeout(resolve, interval - timeSinceLastRequest));
      }
      
      lastRequest = Date.now();
    };
  }
}

// Export utility classes as default
export default {
  VectorUtils,
  FilterBuilder,
  PointBuilder,
  BatchHelper,
  ConfigHelper,
  ValidationUtils,
  PerformanceUtils
};