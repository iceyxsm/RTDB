/**
 * RTDB JavaScript/TypeScript SDK
 * Official client library for RTDB vector database
 * 
 * @version 1.0.0
 * @author RTDB Team
 * @license MIT
 */

// Main client class
export { RTDBClient } from './client';

// HTTP client for advanced usage
export { HttpClient } from './http-client';

// All type definitions
export * from './types';

// Default export for convenience
export { RTDBClient as default } from './client';

// Version information
export const VERSION = '1.0.0';

// Utility functions for common operations
export * from './utils';

/**
 * Create a new RTDB client instance
 * @param config Client configuration
 * @returns RTDBClient instance
 */
export function createClient(config: import('./types').ClientConfig): import('./client').RTDBClient {
  return new (require('./client').RTDBClient)(config);
}

/**
 * Quick connection helper for common use cases
 * @param url RTDB server URL
 * @param apiKey Optional API key
 * @returns RTDBClient instance
 */
export function connect(url: string, apiKey?: string): import('./client').RTDBClient {
  return createClient({ url, apiKey });
}