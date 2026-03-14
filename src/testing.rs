//! Testing utilities and framework for RTDB
//!
//! This module provides testing utilities, mock implementations,
//! and test helpers for RTDB components.

use crate::RTDBError;

/// Test utilities for RTDB components
pub struct TestUtils;

impl TestUtils {
    /// Create a test vector with specified dimension
    pub fn create_test_vector(dimension: usize) -> Vec<f32> {
        (0..dimension).map(|i| i as f32 / dimension as f32).collect()
    }
    
    /// Create random test vector
    pub fn create_random_vector(dimension: usize) -> Vec<f32> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..dimension).map(|_| rng.gen::<f32>()).collect()
    }
}

/// Mock RTDB client for testing
pub struct MockRTDBClient {
    pub responses: std::collections::HashMap<String, Vec<u8>>,
}

impl MockRTDBClient {
    pub fn new() -> Self {
        Self {
            responses: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_response(&mut self, key: String, response: Vec<u8>) {
        self.responses.insert(key, response);
    }
}

/// Test configuration builder
pub struct TestConfigBuilder {
    config: std::collections::HashMap<String, String>,
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: std::collections::HashMap::new(),
        }
    }
    
    pub fn with_setting(mut self, key: &str, value: &str) -> Self {
        self.config.insert(key.to_string(), value.to_string());
        self
    }
    
    pub fn build(self) -> std::collections::HashMap<String, String> {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_vector() {
        let vector = TestUtils::create_test_vector(128);
        assert_eq!(vector.len(), 128);
        assert_eq!(vector[0], 0.0);
        assert_eq!(vector[127], 127.0 / 128.0);
    }

    #[test]
    fn test_mock_client() {
        let mut client = MockRTDBClient::new();
        client.add_response("test".to_string(), vec![1, 2, 3]);
        assert_eq!(client.responses.get("test"), Some(&vec![1, 2, 3]));
    }
}