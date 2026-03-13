/**
 * RTDB JavaScript/TypeScript Client
 * Production-grade vector database client with Qdrant API compatibility
 */

import { HttpClient, HttpResponse } from './http-client';
import {
  ClientConfig,
  CollectionConfig,
  CollectionInfo,
  CollectionsResponse,
  Point,
  UpsertRequest,
  UpsertResponse,
  SearchRequest,
  SearchResponse,
  BatchSearchRequest,
  BatchSearchResponse,
  QueryRequest,
  RetrieveRequest,
  RetrieveResponse,
  DeleteRequest,
  DeleteResponse,
  ScrollRequest,
  ScrollResponse,
  CountRequest,
  CountResponse,
  SnapshotsResponse,
  HealthResponse,
  ServiceInfo,
  TelemetryData,
  RequestOptions,
  VectorId,
  RTDBError,
  ValidationError,
  NotFoundError,
  ConflictError
} from './types';

/**
 * Main RTDB client class with full Qdrant API compatibility
 */
export class RTDBClient {
  private http: HttpClient;
  private config: ClientConfig;

  constructor(config: ClientConfig) {
    this.config = this.validateConfig(config);
    this.http = new HttpClient(this.config);
  }

  /**
   * Validate client configuration
   */
  private validateConfig(config: ClientConfig): ClientConfig {
    if (!config.url) {
      throw new ValidationError('URL is required');
    }

    // Ensure URL has protocol
    if (!config.url.startsWith('http://') && !config.url.startsWith('https://')) {
      config.url = `http://${config.url}`;
    }

    // Remove trailing slash
    config.url = config.url.replace(/\/$/, '');

    // Add port if specified
    if (config.port && !config.url.includes(':' + config.port)) {
      const url = new URL(config.url);
      url.port = config.port.toString();
      config.url = url.toString().replace(/\/$/, '');
    }

    return {
      timeout: 30000,
      retries: 3,
      retryDelay: 100,
      maxRetryDelay: 5000,
      retryMultiplier: 2,
      ...config
    };
  }

  // ============================================================================
  // SERVICE ENDPOINTS
  // ============================================================================

  /**
   * Get service health information
   */
  async healthCheck(options?: RequestOptions): Promise<HealthResponse> {
    const response = await this.http.get<HealthResponse>('/', options);
    return response.data;
  }

  /**
   * Get service information
   */
  async getServiceInfo(options?: RequestOptions): Promise<ServiceInfo> {
    const response = await this.http.get<ServiceInfo>('/', options);
    return response.data;
  }

  /**
   * Check if service is ready
   */
  async isReady(options?: RequestOptions): Promise<boolean> {
    try {
      await this.http.get('/readyz', options);
      return true;
    } catch (error) {
      return false;
    }
  }

  /**
   * Check if service is alive
   */
  async isAlive(options?: RequestOptions): Promise<boolean> {
    try {
      await this.http.get('/livez', options);
      return true;
    } catch (error) {
      return false;
    }
  }

  /**
   * Get telemetry data
   */
  async getTelemetry(options?: RequestOptions): Promise<TelemetryData> {
    const response = await this.http.get<TelemetryData>('/telemetry', options);
    return response.data;
  }

  // ============================================================================
  // COLLECTION MANAGEMENT
  // ============================================================================

  /**
   * List all collections
   */
  async listCollections(options?: RequestOptions): Promise<string[]> {
    const response = await this.http.get<CollectionsResponse>('/collections', options);
    return response.data.collections.map(c => c.name);
  }

  /**
   * Create a new collection
   */
  async createCollection(
    name: string,
    config: CollectionConfig,
    options?: RequestOptions
  ): Promise<boolean> {
    this.validateCollectionName(name);
    this.validateCollectionConfig(config);

    const response = await this.http.put<{ result: boolean }>(
      `/collections/${encodeURIComponent(name)}`,
      config,
      options
    );

    return response.data.result;
  }

  /**
   * Get collection information
   */
  async getCollection(name: string, options?: RequestOptions): Promise<CollectionInfo> {
    this.validateCollectionName(name);

    const response = await this.http.get<{ result: CollectionInfo }>(
      `/collections/${encodeURIComponent(name)}`,
      options
    );

    return response.data.result;
  }

  /**
   * Check if collection exists
   */
  async collectionExists(name: string, options?: RequestOptions): Promise<boolean> {
    this.validateCollectionName(name);

    try {
      const response = await this.http.get<{ result: { exists: boolean } }>(
        `/collections/${encodeURIComponent(name)}/exists`,
        options
      );
      return response.data.result.exists;
    } catch (error: any) {
      if (error.status === 404) {
        return false;
      }
      throw error;
    }
  }

  /**
   * Delete a collection
   */
  async deleteCollection(name: string, options?: RequestOptions): Promise<boolean> {
    this.validateCollectionName(name);

    const response = await this.http.delete<{ result: boolean }>(
      `/collections/${encodeURIComponent(name)}`,
      options
    );

    return response.data.result;
  }

  /**
   * Update collection configuration
   */
  async updateCollection(
    name: string,
    config: Partial<CollectionConfig>,
    options?: RequestOptions
  ): Promise<boolean> {
    this.validateCollectionName(name);

    const response = await this.http.put<{ result: boolean }>(
      `/collections/${encodeURIComponent(name)}`,
      config,
      options
    );

    return response.data.result;
  }

  // ============================================================================
  // POINT OPERATIONS
  // ============================================================================

  /**
   * Upsert points into collection
   */
  async upsert(
    collectionName: string,
    request: UpsertRequest,
    options?: RequestOptions
  ): Promise<UpsertResponse> {
    this.validateCollectionName(collectionName);
    this.validateUpsertRequest(request);

    const response = await this.http.put<{ result: UpsertResponse }>(
      `/collections/${encodeURIComponent(collectionName)}/points`,
      request,
      options
    );

    return response.data.result;
  }

  /**
   * Search for similar vectors
   */
  async search(
    collectionName: string,
    request: SearchRequest,
    options?: RequestOptions
  ): Promise<SearchResponse> {
    this.validateCollectionName(collectionName);
    this.validateSearchRequest(request);

    const response = await this.http.post<SearchResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/search`,
      request,
      options
    );

    return response.data;
  }

  /**
   * Batch search for multiple queries
   */
  async searchBatch(
    collectionName: string,
    request: BatchSearchRequest,
    options?: RequestOptions
  ): Promise<BatchSearchResponse> {
    this.validateCollectionName(collectionName);
    this.validateBatchSearchRequest(request);

    const response = await this.http.post<BatchSearchResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/search/batch`,
      request,
      options
    );

    return response.data;
  }

  /**
   * Query points with advanced filtering
   */
  async query(
    collectionName: string,
    request: QueryRequest,
    options?: RequestOptions
  ): Promise<SearchResponse> {
    this.validateCollectionName(collectionName);

    const response = await this.http.post<SearchResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/query`,
      request,
      options
    );

    return response.data;
  }

  /**
   * Retrieve points by IDs
   */
  async retrieve(
    collectionName: string,
    request: RetrieveRequest,
    options?: RequestOptions
  ): Promise<RetrieveResponse> {
    this.validateCollectionName(collectionName);
    this.validateRetrieveRequest(request);

    const response = await this.http.post<RetrieveResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/retrieve`,
      request,
      options
    );

    return response.data;
  }

  /**
   * Get a single point by ID
   */
  async getPoint(
    collectionName: string,
    id: VectorId,
    withPayload: boolean = true,
    withVector: boolean = false,
    options?: RequestOptions
  ): Promise<Point | null> {
    this.validateCollectionName(collectionName);

    try {
      const params: Record<string, string> = {};
      if (!withPayload) params.with_payload = 'false';
      if (withVector) params.with_vector = 'true';

      const response = await this.http.get<{ result: Point }>(
        `/collections/${encodeURIComponent(collectionName)}/points/${encodeURIComponent(String(id))}`,
        { ...options, params }
      );

      return response.data.result;
    } catch (error: any) {
      if (error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  /**
   * Delete points from collection
   */
  async delete(
    collectionName: string,
    request: DeleteRequest,
    options?: RequestOptions
  ): Promise<DeleteResponse> {
    this.validateCollectionName(collectionName);
    this.validateDeleteRequest(request);

    const response = await this.http.post<{ result: DeleteResponse }>(
      `/collections/${encodeURIComponent(collectionName)}/points/delete`,
      request,
      options
    );

    return response.data.result;
  }

  /**
   * Delete a single point by ID
   */
  async deletePoint(
    collectionName: string,
    id: VectorId,
    options?: RequestOptions
  ): Promise<boolean> {
    this.validateCollectionName(collectionName);

    const response = await this.http.delete<{ result: boolean }>(
      `/collections/${encodeURIComponent(collectionName)}/points/${encodeURIComponent(String(id))}`,
      options
    );

    return response.data.result;
  }

  /**
   * Scroll through points in collection
   */
  async scroll(
    collectionName: string,
    request: ScrollRequest = {},
    options?: RequestOptions
  ): Promise<ScrollResponse> {
    this.validateCollectionName(collectionName);

    const response = await this.http.post<ScrollResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/scroll`,
      request,
      options
    );

    return response.data;
  }

  /**
   * Count points in collection
   */
  async count(
    collectionName: string,
    request: CountRequest = {},
    options?: RequestOptions
  ): Promise<number> {
    this.validateCollectionName(collectionName);

    const response = await this.http.post<CountResponse>(
      `/collections/${encodeURIComponent(collectionName)}/points/count`,
      request,
      options
    );

    return response.data.result.count;
  }

  // ============================================================================
  // SNAPSHOT OPERATIONS
  // ============================================================================

  /**
   * List collection snapshots
   */
  async listSnapshots(collectionName: string, options?: RequestOptions): Promise<string[]> {
    this.validateCollectionName(collectionName);

    const response = await this.http.get<SnapshotsResponse>(
      `/collections/${encodeURIComponent(collectionName)}/snapshots`,
      options
    );

    return response.data.result.map(s => s.name);
  }

  /**
   * Create collection snapshot
   */
  async createSnapshot(collectionName: string, options?: RequestOptions): Promise<string> {
    this.validateCollectionName(collectionName);

    const response = await this.http.post<{ result: { name: string } }>(
      `/collections/${encodeURIComponent(collectionName)}/snapshots`,
      {},
      options
    );

    return response.data.result.name;
  }

  /**
   * Delete collection snapshot
   */
  async deleteSnapshot(
    collectionName: string,
    snapshotName: string,
    options?: RequestOptions
  ): Promise<boolean> {
    this.validateCollectionName(collectionName);

    const response = await this.http.delete<{ result: boolean }>(
      `/collections/${encodeURIComponent(collectionName)}/snapshots/${encodeURIComponent(snapshotName)}`,
      options
    );

    return response.data.result;
  }

  /**
   * List full database snapshots
   */
  async listFullSnapshots(options?: RequestOptions): Promise<string[]> {
    const response = await this.http.get<SnapshotsResponse>('/snapshots', options);
    return response.data.result.map(s => s.name);
  }

  /**
   * Create full database snapshot
   */
  async createFullSnapshot(options?: RequestOptions): Promise<string> {
    const response = await this.http.post<{ result: { name: string } }>('/snapshots', {}, options);
    return response.data.result.name;
  }

  // ============================================================================
  // VALIDATION METHODS
  // ============================================================================

  private validateCollectionName(name: string): void {
    if (!name || typeof name !== 'string') {
      throw new ValidationError('Collection name must be a non-empty string');
    }
    if (name.length > 255) {
      throw new ValidationError('Collection name must be less than 255 characters');
    }
    if (!/^[a-zA-Z0-9_-]+$/.test(name)) {
      throw new ValidationError('Collection name can only contain letters, numbers, underscores, and hyphens');
    }
  }

  private validateCollectionConfig(config: CollectionConfig): void {
    if (!config.vector_size || config.vector_size <= 0) {
      throw new ValidationError('vector_size must be a positive integer');
    }
    if (config.vector_size > 65536) {
      throw new ValidationError('vector_size must be less than 65536');
    }
    if (!config.distance || !['Cosine', 'Euclidean', 'Dot', 'Manhattan'].includes(config.distance)) {
      throw new ValidationError('distance must be one of: Cosine, Euclidean, Dot, Manhattan');
    }
  }

  private validateUpsertRequest(request: UpsertRequest): void {
    if (!request.points || !Array.isArray(request.points) || request.points.length === 0) {
      throw new ValidationError('points must be a non-empty array');
    }
    if (request.points.length > 10000) {
      throw new ValidationError('Cannot upsert more than 10,000 points at once');
    }

    for (const point of request.points) {
      if (!point.vector || !Array.isArray(point.vector)) {
        throw new ValidationError('Each point must have a vector array');
      }
      if (point.id === undefined || point.id === null) {
        throw new ValidationError('Each point must have an id');
      }
    }
  }

  private validateSearchRequest(request: SearchRequest): void {
    if (!request.vector || !Array.isArray(request.vector)) {
      throw new ValidationError('vector must be an array');
    }
    if (request.limit !== undefined && (request.limit <= 0 || request.limit > 10000)) {
      throw new ValidationError('limit must be between 1 and 10,000');
    }
    if (request.offset !== undefined && request.offset < 0) {
      throw new ValidationError('offset must be non-negative');
    }
  }

  private validateBatchSearchRequest(request: BatchSearchRequest): void {
    if (!request.searches || !Array.isArray(request.searches) || request.searches.length === 0) {
      throw new ValidationError('searches must be a non-empty array');
    }
    if (request.searches.length > 100) {
      throw new ValidationError('Cannot perform more than 100 searches at once');
    }

    for (const search of request.searches) {
      this.validateSearchRequest(search);
    }
  }

  private validateRetrieveRequest(request: RetrieveRequest): void {
    if (!request.ids || !Array.isArray(request.ids) || request.ids.length === 0) {
      throw new ValidationError('ids must be a non-empty array');
    }
    if (request.ids.length > 10000) {
      throw new ValidationError('Cannot retrieve more than 10,000 points at once');
    }
  }

  private validateDeleteRequest(request: DeleteRequest): void {
    if (!request.points && !request.filter) {
      throw new ValidationError('Either points or filter must be specified');
    }
    if (request.points && request.points.length > 10000) {
      throw new ValidationError('Cannot delete more than 10,000 points at once');
    }
  }

  // ============================================================================
  // UTILITY METHODS
  // ============================================================================

  /**
   * Get client configuration
   */
  getConfig(): ClientConfig {
    return { ...this.config };
  }

  /**
   * Close client and cleanup resources
   */
  close(): void {
    this.http.destroy();
  }

  /**
   * Create a new client instance with the same configuration
   */
  clone(overrides?: Partial<ClientConfig>): RTDBClient {
    return new RTDBClient({ ...this.config, ...overrides });
  }
}