/**
 * RTDB JavaScript/TypeScript SDK Types
 * Production-grade type definitions for vector database operations
 */

// Core vector and point types
export type VectorId = string | number;
export type Vector = number[];
export type Payload = Record<string, any>;

// Distance metrics supported by RTDB
export type DistanceMetric = 'Cosine' | 'Euclidean' | 'Dot' | 'Manhattan';

// Collection configuration
export interface CollectionConfig {
  vector_size: number;
  distance: DistanceMetric;
  hnsw_config?: HnswConfig;
  quantization_config?: QuantizationConfig;
  on_disk_payload?: boolean;
}

// HNSW index configuration
export interface HnswConfig {
  m?: number;
  ef_construct?: number;
  full_scan_threshold?: number;
  max_indexing_threads?: number;
  on_disk?: boolean;
}

// Quantization configuration
export interface QuantizationConfig {
  scalar?: ScalarQuantization;
  product?: ProductQuantization;
  binary?: BinaryQuantization;
}

export interface ScalarQuantization {
  type: 'int8';
  quantile?: number;
  always_ram?: boolean;
}

export interface ProductQuantization {
  compression: 'x4' | 'x8' | 'x16' | 'x32' | 'x64';
  always_ram?: boolean;
}

export interface BinaryQuantization {
  always_ram?: boolean;
}

// Point operations
export interface Point {
  id: VectorId;
  vector: Vector;
  payload?: Payload;
}

export interface UpsertRequest {
  points: Point[];
  wait?: boolean;
  ordering?: WriteOrdering;
}

export interface UpsertResponse {
  operation_id: number;
  status: OperationStatus;
}

export type WriteOrdering = 'weak' | 'medium' | 'strong';
export type OperationStatus = 'acknowledged' | 'completed';

// Search operations
export interface SearchRequest {
  vector: Vector;
  limit?: number;
  offset?: number;
  filter?: Filter;
  params?: SearchParams;
  score_threshold?: number;
  with_payload?: boolean | string[];
  with_vector?: boolean;
}

export interface SearchParams {
  hnsw_ef?: number;
  exact?: boolean;
  quantization?: QuantizationSearchParams;
}

export interface QuantizationSearchParams {
  ignore?: boolean;
  rescore?: boolean;
  oversampling?: number;
}

export interface SearchResult {
  id: VectorId;
  score: number;
  payload?: Payload;
  vector?: Vector;
}

export interface SearchResponse {
  result: SearchResult[];
  status: string;
  time: number;
}

// Batch search
export interface BatchSearchRequest {
  searches: SearchRequest[];
}

export interface BatchSearchResponse {
  result: SearchResult[][];
  status: string;
  time: number;
}

// Query operations (more flexible than search)
export interface QueryRequest {
  query?: Vector;
  prefetch?: PrefetchQuery[];
  using?: string;
  filter?: Filter;
  params?: SearchParams;
  limit?: number;
  offset?: number;
  with_payload?: boolean | string[];
  with_vector?: boolean;
  score_threshold?: number;
}

export interface PrefetchQuery {
  prefetch?: PrefetchQuery[];
  query?: Vector;
  using?: string;
  filter?: Filter;
  params?: SearchParams;
  limit?: number;
  score_threshold?: number;
}

// Filtering system
export interface Filter {
  should?: Condition[];
  must?: Condition[];
  must_not?: Condition[];
}

export type Condition = FieldCondition | HasIdCondition | IsEmptyCondition | IsNullCondition;

export interface FieldCondition {
  key: string;
  match?: MatchValue;
  range?: RangeCondition;
  geo_bounding_box?: GeoBoundingBox;
  geo_radius?: GeoRadius;
  values_count?: ValuesCount;
}

export interface HasIdCondition {
  has_id: VectorId[];
}

export interface IsEmptyCondition {
  is_empty: {
    key: string;
  };
}

export interface IsNullCondition {
  is_null: {
    key: string;
  };
}

export type MatchValue = MatchText | MatchInteger | MatchKeyword | MatchBool | MatchAny;

export interface MatchText {
  text: string;
}

export interface MatchInteger {
  integer: number;
}

export interface MatchKeyword {
  keyword: string;
}

export interface MatchBool {
  boolean: boolean;
}

export interface MatchAny {
  any: (string | number | boolean)[];
}

export interface RangeCondition {
  lt?: number;
  gt?: number;
  gte?: number;
  lte?: number;
}

export interface GeoBoundingBox {
  top_left: GeoPoint;
  bottom_right: GeoPoint;
}

export interface GeoRadius {
  center: GeoPoint;
  radius: number;
}

export interface GeoPoint {
  lon: number;
  lat: number;
}

export interface ValuesCount {
  lt?: number;
  gt?: number;
  gte?: number;
  lte?: number;
}

// Collection operations
export interface CollectionInfo {
  status: string;
  optimizer_status: string;
  vectors_count: number;
  indexed_vectors_count: number;
  points_count: number;
  segments_count: number;
  config: CollectionConfig;
  payload_schema: Record<string, PayloadSchemaInfo>;
}

export interface PayloadSchemaInfo {
  data_type: string;
  params?: Record<string, any>;
  points?: number;
}

export interface CollectionDescription {
  name: string;
}

export interface CollectionsResponse {
  collections: CollectionDescription[];
}

// Retrieve operations
export interface RetrieveRequest {
  ids: VectorId[];
  with_payload?: boolean | string[];
  with_vector?: boolean;
}

export interface RetrieveResponse {
  result: Point[];
  status: string;
  time: number;
}

// Delete operations
export interface DeleteRequest {
  points?: VectorId[];
  filter?: Filter;
  wait?: boolean;
  ordering?: WriteOrdering;
}

export interface DeleteResponse {
  operation_id: number;
  status: OperationStatus;
}

// Scroll operations
export interface ScrollRequest {
  filter?: Filter;
  limit?: number;
  offset?: VectorId;
  with_payload?: boolean | string[];
  with_vector?: boolean;
  order_by?: OrderBy;
}

export interface OrderBy {
  key: string;
  direction?: 'asc' | 'desc';
  start_from?: string | number;
}

export interface ScrollResponse {
  result: {
    points: Point[];
    next_page_offset?: VectorId;
  };
  status: string;
  time: number;
}

// Count operations
export interface CountRequest {
  filter?: Filter;
  exact?: boolean;
}

export interface CountResponse {
  result: {
    count: number;
  };
  status: string;
  time: number;
}

// Snapshot operations
export interface SnapshotDescription {
  name: string;
  creation_time?: string;
  size?: number;
}

export interface SnapshotsResponse {
  result: SnapshotDescription[];
  status: string;
  time: number;
}

// Client configuration
export interface ClientConfig {
  url: string;
  apiKey?: string;
  timeout?: number;
  retries?: number;
  retryDelay?: number;
  maxRetryDelay?: number;
  retryMultiplier?: number;
  headers?: Record<string, string>;
  https?: {
    rejectUnauthorized?: boolean;
    ca?: string;
    cert?: string;
    key?: string;
  };
  grpc?: boolean;
  prefix?: string;
  port?: number;
}

// Connection pool configuration
export interface ConnectionPoolConfig {
  maxConnections?: number;
  keepAlive?: boolean;
  keepAliveTimeout?: number;
  maxIdleTime?: number;
  connectionTimeout?: number;
}

// Error types
export class RTDBError extends Error {
  public readonly code: string;
  public readonly status?: number;
  public readonly details?: any;

  constructor(message: string, code: string, status?: number, details?: any) {
    super(message);
    this.name = 'RTDBError';
    this.code = code;
    this.status = status;
    this.details = details;
  }
}

export class ConnectionError extends RTDBError {
  constructor(message: string, details?: any) {
    super(message, 'CONNECTION_ERROR', undefined, details);
    this.name = 'ConnectionError';
  }
}

export class TimeoutError extends RTDBError {
  constructor(message: string, details?: any) {
    super(message, 'TIMEOUT_ERROR', undefined, details);
    this.name = 'TimeoutError';
  }
}

export class ValidationError extends RTDBError {
  constructor(message: string, details?: any) {
    super(message, 'VALIDATION_ERROR', 400, details);
    this.name = 'ValidationError';
  }
}

export class NotFoundError extends RTDBError {
  constructor(message: string, details?: any) {
    super(message, 'NOT_FOUND', 404, details);
    this.name = 'NotFoundError';
  }
}

export class ConflictError extends RTDBError {
  constructor(message: string, details?: any) {
    super(message, 'CONFLICT', 409, details);
    this.name = 'ConflictError';
  }
}

// Health check types
export interface HealthResponse {
  title: string;
  version: string;
  commit?: string;
}

// Service info types
export interface ServiceInfo {
  title: string;
  version: string;
  commit?: string;
}

// Telemetry types
export interface TelemetryData {
  app: {
    name: string;
    version: string;
    commit?: string;
  };
  collections: {
    [key: string]: {
      vectors_count: number;
      segments_count: number;
      disk_data_size: number;
      ram_data_size: number;
    };
  };
  cluster?: {
    status: string;
    peer_count: number;
    pending_operations: number;
    consensus_thread_status: string;
  };
}

// Request options for individual operations
export interface RequestOptions {
  timeout?: number;
  signal?: AbortSignal;
  headers?: Record<string, string>;
}

// Utility types
export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

export type RequiredFields<T, K extends keyof T> = T & Required<Pick<T, K>>;