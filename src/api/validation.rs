//! Production-grade request validation for REST API
//!
//! Provides comprehensive validation for all API requests with:
//! - Input sanitization and bounds checking
//! - Business rule validation
//! - Performance-aware validation (early exits)
//! - Detailed error messages for debugging

use crate::api::error::{ApiError, ValidationError};
use serde_json::Value;

/// Request validation utilities
pub struct RequestValidator;

impl RequestValidator {
    /// Validate collection creation request
    pub fn validate_collection_config(
        name: &str,
        dimension: Option<usize>,
        distance: Option<&str>,
    ) -> Result<(), ApiError> {
        // Validate collection name
        Self::validate_collection_name(name)?;
        
        // Validate dimension
        if let Some(dim) = dimension {
            Self::validate_vector_dimension(dim)?;
        }
        
        // Validate distance metric
        if let Some(dist) = distance {
            Self::validate_distance_metric(dist)?;
        }
        
        Ok(())
    }
    
    /// Validate vector upsert request
    pub fn validate_upsert_request(
        collection_name: &str,
        vectors: &[serde_json::Value],
        expected_dimension: Option<usize>,
    ) -> Result<(), ApiError> {
        // Validate collection name
        Self::validate_collection_name(collection_name)?;
        
        // Validate batch size
        if vectors.is_empty() {
            return Err(ApiError::ValidationFailed {
                errors: vec![ValidationError {
                    field: "points".to_string(),
                    message: "At least one point must be provided".to_string(),
                    code: "REQUIRED".to_string(),
                }]
            });
        }
        
        if vectors.len() > 10000 {
            return Err(ApiError::ValidationFailed {
                errors: vec![ValidationError {
                    field: "points".to_string(),
                    message: "Cannot upsert more than 10,000 points at once".to_string(),
                    code: "MAX_BATCH_SIZE".to_string(),
                }]
            });
        }
        
        // Validate each vector
        let mut errors = Vec::new();
        for (i, vector_data) in vectors.iter().enumerate() {
            if let Err(mut validation_errors) = Self::validate_point_data(vector_data, expected_dimension) {
                // Add index to field names for better error reporting
                for error in &mut validation_errors {
                    error.field = format!("points[{}].{}", i, error.field);
                }
                errors.extend(validation_errors);
            }
        }
        
        if !errors.is_empty() {
            return Err(ApiError::ValidationFailed { errors });
        }
        
        Ok(())
    }
    
    /// Validate search request
    pub fn validate_search_request(
        collection_name: &str,
        vector: &[f32],
        limit: usize,
        offset: Option<usize>,
        expected_dimension: Option<usize>,
    ) -> Result<(), ApiError> {
        // Validate collection name
        Self::validate_collection_name(collection_name)?;
        
        // Validate query vector
        if vector.is_empty() {
            return Err(ApiError::ValidationFailed {
                errors: vec![ValidationError {
                    field: "vector".to_string(),
                    message: "Query vector cannot be empty".to_string(),
                    code: "REQUIRED".to_string(),
                }]
            });
        }
        
        // Validate vector dimension
        if let Some(expected_dim) = expected_dimension {
            if vector.len() != expected_dim {
                return Err(ApiError::InvalidVectorDimension {
                    expected: expected_dim,
                    actual: vector.len(),
                });
            }
        }
        
        // Validate vector values (no NaN, Inf)
        for (i, &value) in vector.iter().enumerate() {
            if !value.is_finite() {
                return Err(ApiError::ValidationFailed {
                    errors: vec![ValidationError {
                        field: format!("vector[{}]", i),
                        message: "Vector values must be finite (no NaN or Infinity)".to_string(),
                        code: "INVALID_VALUE".to_string(),
                    }]
                });
            }
        }
        
        // Validate limit
        Self::validate_limit(limit)?;
        
        // Validate offset
        if let Some(off) = offset {
            if off > 1_000_000 {
                return Err(ApiError::ValidationFailed {
                    errors: vec![ValidationError {
                        field: "offset".to_string(),
                        message: "Offset cannot exceed 1,000,000".to_string(),
                        code: "MAX_VALUE".to_string(),
                    }]
                });
            }
        }
        
        Ok(())
    }
    
    /// Validate individual point data
    fn validate_point_data(
        point: &Value,
        expected_dimension: Option<usize>,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        
        // Validate ID
        if let Some(id_value) = point.get("id") {
            if !Self::is_valid_point_id(id_value) {
                errors.push(ValidationError {
                    field: "id".to_string(),
                    message: "Point ID must be a positive integer or non-empty string".to_string(),
                    code: "INVALID_FORMAT".to_string(),
                });
            }
        } else {
            errors.push(ValidationError {
                field: "id".to_string(),
                message: "Point ID is required".to_string(),
                code: "REQUIRED".to_string(),
            });
        }
        
        // Validate vector
        if let Some(vector_value) = point.get("vector") {
            if let Err(mut vector_errors) = Self::validate_vector_value(vector_value, expected_dimension) {
                errors.append(&mut vector_errors);
            }
        } else {
            errors.push(ValidationError {
                field: "vector".to_string(),
                message: "Vector is required".to_string(),
                code: "REQUIRED".to_string(),
            });
        }
        
        // Validate payload (optional)
        if let Some(payload_value) = point.get("payload") {
            if let Err(mut payload_errors) = Self::validate_payload(payload_value) {
                errors.append(&mut payload_errors);
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Validate vector value (can be array or named vectors object)
    fn validate_vector_value(
        vector: &Value,
        expected_dimension: Option<usize>,
    ) -> Result<(), Vec<ValidationError>> {
        match vector {
            Value::Array(arr) => {
                // Plain vector array
                Self::validate_vector_array(arr, expected_dimension, "vector")
            }
            Value::Object(obj) => {
                // Named vectors
                let mut errors = Vec::new();
                for (name, vec_value) in obj {
                    if let Value::Array(arr) = vec_value {
                        if let Err(mut vec_errors) = Self::validate_vector_array(arr, expected_dimension, &format!("vector.{}", name)) {
                            errors.append(&mut vec_errors);
                        }
                    } else {
                        errors.push(ValidationError {
                            field: format!("vector.{}", name),
                            message: "Named vector must be an array of numbers".to_string(),
                            code: "INVALID_TYPE".to_string(),
                        });
                    }
                }
                
                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors)
                }
            }
            _ => Err(vec![ValidationError {
                field: "vector".to_string(),
                message: "Vector must be an array of numbers or object with named vectors".to_string(),
                code: "INVALID_TYPE".to_string(),
            }])
        }
    }
    
    /// Validate vector array
    fn validate_vector_array(
        arr: &[Value],
        expected_dimension: Option<usize>,
        field_name: &str,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        
        // Check dimension
        if let Some(expected_dim) = expected_dimension {
            if arr.len() != expected_dim {
                errors.push(ValidationError {
                    field: field_name.to_string(),
                    message: format!("Vector dimension must be {} but got {}", expected_dim, arr.len()),
                    code: "INVALID_DIMENSION".to_string(),
                });
                return Err(errors);
            }
        }
        
        // Validate dimension bounds
        if arr.is_empty() {
            errors.push(ValidationError {
                field: field_name.to_string(),
                message: "Vector cannot be empty".to_string(),
                code: "MIN_LENGTH".to_string(),
            });
        } else if arr.len() > 65536 {
            errors.push(ValidationError {
                field: field_name.to_string(),
                message: "Vector dimension cannot exceed 65,536".to_string(),
                code: "MAX_LENGTH".to_string(),
            });
        }
        
        // Validate each component
        for (i, value) in arr.iter().enumerate() {
            match value.as_f64() {
                Some(f) => {
                    if !f.is_finite() {
                        errors.push(ValidationError {
                            field: format!("{}[{}]", field_name, i),
                            message: "Vector components must be finite numbers (no NaN or Infinity)".to_string(),
                            code: "INVALID_VALUE".to_string(),
                        });
                    }
                }
                None => {
                    errors.push(ValidationError {
                        field: format!("{}[{}]", field_name, i),
                        message: "Vector components must be numbers".to_string(),
                        code: "INVALID_TYPE".to_string(),
                    });
                }
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    /// Validate payload object
    fn validate_payload(payload: &Value) -> Result<(), Vec<ValidationError>> {
        match payload {
            Value::Object(_) => {
                // Payload is valid JSON object
                // Additional validation could be added here for:
                // - Maximum nesting depth
                // - Maximum key length
                // - Maximum value size
                // - Forbidden field names
                Ok(())
            }
            Value::Null => Ok(()), // Null payload is allowed
            _ => Err(vec![ValidationError {
                field: "payload".to_string(),
                message: "Payload must be a JSON object or null".to_string(),
                code: "INVALID_TYPE".to_string(),
            }])
        }
    }
    
    /// Check if point ID is valid
    fn is_valid_point_id(id: &Value) -> bool {
        match id {
            Value::Number(n) => {
                // Must be positive integer
                n.as_u64().is_some()
            }
            Value::String(s) => {
                // Must be non-empty string
                !s.is_empty() && s.len() <= 255
            }
            _ => false,
        }
    }
    
    /// Validate collection name
    fn validate_collection_name(name: &str) -> Result<(), ApiError> {
        crate::api::error::validate_collection_name(name)
            .map_err(|e| ApiError::ValidationFailed { errors: vec![e] })
    }
    
    /// Validate vector dimension
    fn validate_vector_dimension(dimension: usize) -> Result<(), ApiError> {
        crate::api::error::validate_vector_dimension(dimension)
            .map_err(|e| ApiError::ValidationFailed { errors: vec![e] })
    }
    
    /// Validate limit parameter
    fn validate_limit(limit: usize) -> Result<(), ApiError> {
        crate::api::error::validate_limit(limit)
            .map_err(|e| ApiError::ValidationFailed { errors: vec![e] })
    }
    
    /// Validate distance metric
    fn validate_distance_metric(distance: &str) -> Result<(), ApiError> {
        match distance.to_lowercase().as_str() {
            "cosine" | "euclidean" | "dot" | "manhattan" => Ok(()),
            _ => Err(ApiError::ValidationFailed {
                errors: vec![ValidationError {
                    field: "distance".to_string(),
                    message: format!("Unsupported distance metric: {}. Supported: cosine, euclidean, dot, manhattan", distance),
                    code: "INVALID_VALUE".to_string(),
                }]
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_collection_config() {
        // Valid config
        assert!(RequestValidator::validate_collection_config("test", Some(128), Some("cosine")).is_ok());
        
        // Invalid name
        assert!(RequestValidator::validate_collection_config("", Some(128), Some("cosine")).is_err());
        
        // Invalid dimension
        assert!(RequestValidator::validate_collection_config("test", Some(0), Some("cosine")).is_err());
        
        // Invalid distance
        assert!(RequestValidator::validate_collection_config("test", Some(128), Some("invalid")).is_err());
    }
    
    #[test]
    fn test_validate_upsert_request() {
        let valid_points = vec![
            json!({
                "id": 1,
                "vector": [0.1, 0.2, 0.3],
                "payload": {"key": "value"}
            })
        ];
        
        // Valid request
        assert!(RequestValidator::validate_upsert_request("test", &valid_points, Some(3)).is_ok());
        
        // Empty points
        assert!(RequestValidator::validate_upsert_request("test", &[], Some(3)).is_err());
        
        // Invalid vector dimension
        let invalid_dim_points = vec![
            json!({
                "id": 1,
                "vector": [0.1, 0.2], // Wrong dimension
                "payload": {"key": "value"}
            })
        ];
        assert!(RequestValidator::validate_upsert_request("test", &invalid_dim_points, Some(3)).is_err());
    }
    
    #[test]
    fn test_validate_search_request() {
        let vector = vec![0.1, 0.2, 0.3];
        
        // Valid request
        assert!(RequestValidator::validate_search_request("test", &vector, 10, None, Some(3)).is_ok());
        
        // Empty vector
        assert!(RequestValidator::validate_search_request("test", &[], 10, None, Some(3)).is_err());
        
        // Invalid dimension
        assert!(RequestValidator::validate_search_request("test", &vector, 10, None, Some(5)).is_err());
        
        // Invalid limit
        assert!(RequestValidator::validate_search_request("test", &vector, 0, None, Some(3)).is_err());
    }
    
    #[test]
    fn test_validate_vector_with_nan() {
        let points = vec![
            json!({
                "id": 1,
                "vector": [0.1, f64::NAN, 0.3],
                "payload": null
            })
        ];
        
        let result = RequestValidator::validate_upsert_request("test", &points, Some(3));
        assert!(result.is_err());
        
        if let Err(ApiError::ValidationFailed { errors }) = result {
            assert!(!errors.is_empty());
            // NaN gets serialized as null by serde_json, so we get a type error
            assert!(errors[0].message.contains("numbers"));
        } else {
            panic!("Expected ValidationFailed error");
        }
    }
    
    #[test]
    fn test_validate_point_id_types() {
        // Valid integer ID
        let point1 = json!({"id": 123, "vector": [0.1, 0.2, 0.3]});
        assert!(RequestValidator::validate_point_data(&point1, Some(3)).is_ok());
        
        // Valid string ID
        let point2 = json!({"id": "uuid-123", "vector": [0.1, 0.2, 0.3]});
        assert!(RequestValidator::validate_point_data(&point2, Some(3)).is_ok());
        
        // Invalid ID (negative)
        let point3 = json!({"id": -1, "vector": [0.1, 0.2, 0.3]});
        assert!(RequestValidator::validate_point_data(&point3, Some(3)).is_err());
        
        // Invalid ID (empty string)
        let point4 = json!({"id": "", "vector": [0.1, 0.2, 0.3]});
        assert!(RequestValidator::validate_point_data(&point4, Some(3)).is_err());
    }
}