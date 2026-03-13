/**
 * Jest test setup file
 */

// Global test timeout
jest.setTimeout(30000);

// Mock fetch for Node.js environment
global.fetch = require('node-fetch');
global.AbortController = require('abort-controller');

// Suppress console logs during tests unless explicitly needed
const originalConsoleLog = console.log;
const originalConsoleWarn = console.warn;
const originalConsoleError = console.error;

beforeEach(() => {
  if (!process.env.VERBOSE_TESTS) {
    console.log = jest.fn();
    console.warn = jest.fn();
    console.error = jest.fn();
  }
});

afterEach(() => {
  if (!process.env.VERBOSE_TESTS) {
    console.log = originalConsoleLog;
    console.warn = originalConsoleWarn;
    console.error = originalConsoleError;
  }
});

// Global test utilities
declare global {
  namespace jest {
    interface Matchers<R> {
      toBeValidVector(dimension?: number): R;
      toBeValidPoint(): R;
    }
  }
}

// Custom matchers
expect.extend({
  toBeValidVector(received: any, dimension?: number) {
    const pass = Array.isArray(received) && 
                 received.length > 0 && 
                 received.every((val: any) => typeof val === 'number' && !isNaN(val)) &&
                 (!dimension || received.length === dimension);

    if (pass) {
      return {
        message: () => `expected ${received} not to be a valid vector`,
        pass: true
      };
    } else {
      return {
        message: () => `expected ${received} to be a valid vector${dimension ? ` with dimension ${dimension}` : ''}`,
        pass: false
      };
    }
  },

  toBeValidPoint(received: any) {
    const pass = typeof received === 'object' &&
                 received !== null &&
                 (typeof received.id === 'string' || typeof received.id === 'number') &&
                 Array.isArray(received.vector) &&
                 received.vector.length > 0;

    if (pass) {
      return {
        message: () => `expected ${received} not to be a valid point`,
        pass: true
      };
    } else {
      return {
        message: () => `expected ${received} to be a valid point`,
        pass: false
      };
    }
  }
});