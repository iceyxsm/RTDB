//! Data validation and integrity checks for migrations
//!
//! Provides comprehensive validation of vector records during migration to ensure
//! data quality and catch issues early in the process.

use crate::migration::{ValidationConfig, VectorRecord};
use crate::{Result, RTDBError};
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// Validate a single vector record
pub fn validate_record(record: &VectorRecord, config: &ValidationConfig) -> Result<()> {
    // Validate vector if enabled
    if config.validate_vectors {
        validate_vector(&record.vector, config.vector_dimension)?;
    }

    // Validate metadata if enabled
    if config.validate_metadata {
        validate_metadata(&record.metadata, &config.required_fields)?;
    }

    // Validate ID
    validate_id(&record.id)?;

    Ok(())
}

/// Validate vector data
fn validate_vector(vector: &[f32], expected_dimension: Option<usize>) -> Result<()> {
    // Check if vector is empty
    if vector.is_empty() {
        return Err(RTDBError::Validation("Vector cannot be empty".to_string()));
    }

    // Check dimension if specified
    if let Some(expected_dim) = expected_dimension {
        if vector.len() != expected_dim {
            return Err(RTDBError::Validation(format!(
                "Vector dimension mismatch: expected {}, got {}",
                expected_dim,
                vector.len()
            )));
        }
    }

    // Check for invalid values (NaN, infinity)
    for (i, &value) in vector.iter().enumerate() {
        if !value.is_finite() {
            return Err(RTDBError::Validation(format!(
                "Invalid vector value at index {}: {}",
                i, value
            )));
        }
    }

    // Check for zero vectors (might indicate data issues)
    let is_zero_vector = vector.iter().all(|&x| x == 0.0);
    if is_zero_vector {
        tracing::warn!("Zero vector detected - this might indicate data quality issues");
    }

    // Check vector magnitude (warn if too large or too small)
    let magnitude: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
    if magnitude < 1e-6 {
        tracing::warn!("Very small vector magnitude: {} - might indicate normalization issues", magnitude);
    } else if magnitude > 1000.0 {
        tracing::warn!("Very large vector magnitude: {} - might indicate scaling issues", magnitude);
    }

    Ok(())
}

/// Validate metadata
fn validate_metadata(
    metadata: &HashMap<String, serde_json::Value>,
    required_fields: &[String],
) -> Result<()> {
    // Check required fields
    for field in required_fields {
        if !metadata.contains_key(field) {
            return Err(RTDBError::Validation(format!(
                "Required field '{}' is missing from metadata",
                field
            )));
        }
    }

    // Validate field names (no special characters that might cause issues)
    let field_name_regex = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
    for field_name in metadata.keys() {
        if !field_name_regex.is_match(field_name) {
            return Err(RTDBError::Validation(format!(
                "Invalid field name '{}': must start with letter or underscore, contain only alphanumeric characters and underscores",
                field_name
            )));
        }
    }

    // Check for excessively large metadata
    let metadata_size = estimate_metadata_size(metadata);
    if metadata_size > 1024 * 1024 {  // 1MB limit
        return Err(RTDBError::Validation(format!(
            "Metadata too large: {} bytes (limit: 1MB)",
            metadata_size
        )));
    }

    // Validate individual field values
    for (field_name, value) in metadata {
        validate_metadata_value(field_name, value)?;
    }

    Ok(())
}

/// Validate individual metadata value
fn validate_metadata_value(field_name: &str, value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::String(s) => {
            if s.len() > 10000 {  // 10KB limit for strings
                return Err(RTDBError::Validation(format!(
                    "String field '{}' too long: {} characters (limit: 10000)",
                    field_name, s.len()
                )));
            }
        }
        serde_json::Value::Array(arr) => {
            if arr.len() > 1000 {  // 1000 element limit for arrays
                return Err(RTDBError::Validation(format!(
                    "Array field '{}' too long: {} elements (limit: 1000)",
                    field_name, arr.len()
                )));
            }
        }
        serde_json::Value::Object(obj) => {
            if obj.len() > 100 {  // 100 key limit for nested objects
                return Err(RTDBError::Validation(format!(
                    "Object field '{}' has too many keys: {} (limit: 100)",
                    field_name, obj.len()
                )));
            }
        }
        _ => {} // Numbers, booleans, null are fine
    }
    Ok(())
}

/// Validate record ID
fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(RTDBError::Validation("Record ID cannot be empty".to_string()));
    }

    if id.len() > 255 {
        return Err(RTDBError::Validation(format!(
            "Record ID too long: {} characters (limit: 255)",
            id.len()
        )));
    }

    // Check for invalid characters that might cause issues in URLs or file systems
    let invalid_chars = ['/', '\\', '?', '%', '*', ':', '|', '"', '<', '>', '\0'];
    for &invalid_char in &invalid_chars {
        if id.contains(invalid_char) {
            return Err(RTDBError::Validation(format!(
                "Record ID contains invalid character: '{}'",
                invalid_char
            )));
        }
    }

    Ok(())
}

/// Estimate metadata size in bytes
fn estimate_metadata_size(metadata: &HashMap<String, serde_json::Value>) -> usize {
    serde_json::to_string(metadata)
        .map(|s| s.len())
        .unwrap_or(0)
}

/// Batch validator for checking multiple records
pub struct BatchValidator {
    config: ValidationConfig,
    duplicate_checker: Option<DuplicateChecker>,
    stats: ValidationStats,
}

impl BatchValidator {
    /// Create new batch validator
    pub fn new(config: ValidationConfig) -> Self {
        let duplicate_checker = if config.check_duplicates {
            Some(DuplicateChecker::new())
        } else {
            None
        };

        Self {
            config,
            duplicate_checker,
            stats: ValidationStats::new(),
        }
    }

    /// Validate a batch of records
    pub fn validate_batch(&mut self, records: &[VectorRecord]) -> Result<ValidationResult> {
        let mut valid_records = Vec::new();
        let mut invalid_records = Vec::new();
        let mut duplicate_records = Vec::new();

        for record in records {
            // Check for duplicates first
            if let Some(ref mut checker) = self.duplicate_checker {
                if checker.is_duplicate(&record.id) {
                    duplicate_records.push(record.clone());
                    self.stats.duplicate_count += 1;
                    continue;
                }
                checker.add_id(&record.id);
            }

            // Validate record
            match validate_record(record, &self.config) {
                Ok(()) => {
                    valid_records.push(record.clone());
                    self.stats.valid_count += 1;
                }
                Err(e) => {
                    invalid_records.push(InvalidRecord {
                        record: record.clone(),
                        error: e.to_string(),
                    });
                    self.stats.invalid_count += 1;
                }
            }
        }

        self.stats.total_processed += records.len() as u64;

        Ok(ValidationResult {
            valid_records,
            invalid_records,
            duplicate_records,
            stats: self.stats.clone(),
        })
    }

    /// Get current validation statistics
    pub fn get_stats(&self) -> &ValidationStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ValidationStats::new();
        if let Some(ref mut checker) = self.duplicate_checker {
            checker.clear();
        }
    }
}

/// Duplicate checker using bloom filter for memory efficiency
pub struct DuplicateChecker {
    seen_ids: HashSet<String>,
    max_ids: usize,
}

impl DuplicateChecker {
    fn new() -> Self {
        Self {
            seen_ids: HashSet::new(),
            max_ids: 1_000_000, // Limit memory usage
        }
    }

    fn is_duplicate(&self, id: &str) -> bool {
        self.seen_ids.contains(id)
    }

    fn add_id(&mut self, id: &str) {
        if self.seen_ids.len() < self.max_ids {
            self.seen_ids.insert(id.to_string());
        } else {
            tracing::warn!("Duplicate checker reached maximum capacity, some duplicates may not be detected");
        }
    }

    fn clear(&mut self) {
        self.seen_ids.clear();
    }
}

/// Validation statistics
#[derive(Debug, Clone)]
pub struct ValidationStats {
    pub total_processed: u64,
    pub valid_count: u64,
    pub invalid_count: u64,
    pub duplicate_count: u64,
}

impl ValidationStats {
    fn new() -> Self {
        Self {
            total_processed: 0,
            valid_count: 0,
            invalid_count: 0,
            duplicate_count: 0,
        }
    }

    /// Get validation success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_processed > 0 {
            self.valid_count as f64 / self.total_processed as f64 * 100.0
        } else {
            0.0
        }
    }

    /// Get error rate as percentage
    pub fn error_rate(&self) -> f64 {
        if self.total_processed > 0 {
            self.invalid_count as f64 / self.total_processed as f64 * 100.0
        } else {
            0.0
        }
    }

    /// Get duplicate rate as percentage
    pub fn duplicate_rate(&self) -> f64 {
        if self.total_processed > 0 {
            self.duplicate_count as f64 / self.total_processed as f64 * 100.0
        } else {
            0.0
        }
    }
}

/// Result of batch validation
pub struct ValidationResult {
    pub valid_records: Vec<VectorRecord>,
    pub invalid_records: Vec<InvalidRecord>,
    pub duplicate_records: Vec<VectorRecord>,
    pub stats: ValidationStats,
}

/// Invalid record with error information
#[derive(Debug, Clone)]
pub struct InvalidRecord {
    pub record: VectorRecord,
    pub error: String,
}

/// Schema validator for ensuring consistent data structure
pub struct SchemaValidator {
    expected_fields: HashSet<String>,
    field_types: HashMap<String, FieldType>,
    strict_mode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

impl SchemaValidator {
    /// Create new schema validator
    pub fn new(strict_mode: bool) -> Self {
        Self {
            expected_fields: HashSet::new(),
            field_types: HashMap::new(),
            strict_mode,
        }
    }

    /// Add expected field with type
    pub fn add_field(&mut self, name: String, field_type: FieldType) {
        self.expected_fields.insert(name.clone());
        self.field_types.insert(name, field_type);
    }

    /// Validate record against schema
    pub fn validate_schema(&self, record: &VectorRecord) -> Result<()> {
        // Check required fields
        for expected_field in &self.expected_fields {
            if !record.metadata.contains_key(expected_field) {
                return Err(RTDBError::Validation(format!(
                    "Missing required field: {}",
                    expected_field
                )));
            }
        }

        // Check field types
        for (field_name, value) in &record.metadata {
            if let Some(expected_type) = self.field_types.get(field_name) {
                let actual_type = get_value_type(value);
                if &actual_type != expected_type {
                    return Err(RTDBError::Validation(format!(
                        "Field '{}' type mismatch: expected {:?}, got {:?}",
                        field_name, expected_type, actual_type
                    )));
                }
            } else if self.strict_mode {
                return Err(RTDBError::Validation(format!(
                    "Unexpected field in strict mode: {}",
                    field_name
                )));
            }
        }

        Ok(())
    }
}

/// Get the type of a JSON value
fn get_value_type(value: &serde_json::Value) -> FieldType {
    match value {
        serde_json::Value::String(_) => FieldType::String,
        serde_json::Value::Number(_) => FieldType::Number,
        serde_json::Value::Bool(_) => FieldType::Boolean,
        serde_json::Value::Array(_) => FieldType::Array,
        serde_json::Value::Object(_) => FieldType::Object,
        serde_json::Value::Null => FieldType::String, // Treat null as string for flexibility
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_vector() {
        // Valid vector
        let vector = vec![1.0, 2.0, 3.0];
        assert!(validate_vector(&vector, Some(3)).is_ok());

        // Wrong dimension
        assert!(validate_vector(&vector, Some(4)).is_err());

        // Empty vector
        assert!(validate_vector(&[], None).is_err());

        // NaN value
        let invalid_vector = vec![1.0, f32::NAN, 3.0];
        assert!(validate_vector(&invalid_vector, None).is_err());

        // Infinity value
        let invalid_vector = vec![1.0, f32::INFINITY, 3.0];
        assert!(validate_vector(&invalid_vector, None).is_err());
    }

    #[test]
    fn test_validate_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), json!("Test Document"));
        metadata.insert("score".to_string(), json!(0.95));

        let required_fields = vec!["title".to_string()];
        assert!(validate_metadata(&metadata, &required_fields).is_ok());

        // Missing required field
        let required_fields = vec!["missing_field".to_string()];
        assert!(validate_metadata(&metadata, &required_fields).is_err());

        // Invalid field name
        metadata.insert("invalid-field-name".to_string(), json!("value"));
        assert!(validate_metadata(&metadata, &[]).is_err());
    }

    #[test]
    fn test_validate_id() {
        // Valid IDs
        assert!(validate_id("doc123").is_ok());
        assert!(validate_id("user_456").is_ok());

        // Invalid IDs
        assert!(validate_id("").is_err()); // Empty
        assert!(validate_id("doc/123").is_err()); // Contains slash
        assert!(validate_id("doc?123").is_err()); // Contains question mark
        assert!(validate_id(&"x".repeat(256)).is_err()); // Too long
    }

    #[test]
    fn test_batch_validator() {
        let config = ValidationConfig {
            validate_vectors: true,
            validate_metadata: true,
            check_duplicates: true,
            vector_dimension: Some(3),
            required_fields: vec!["title".to_string()],
        };

        let mut validator = BatchValidator::new(config);

        let records = vec![
            VectorRecord {
                id: "1".to_string(),
                vector: vec![1.0, 2.0, 3.0],
                metadata: {
                    let mut map = HashMap::new();
                    map.insert("title".to_string(), json!("Doc 1"));
                    map
                },
            },
            VectorRecord {
                id: "1".to_string(), // Duplicate ID
                vector: vec![4.0, 5.0, 6.0],
                metadata: {
                    let mut map = HashMap::new();
                    map.insert("title".to_string(), json!("Doc 1 Duplicate"));
                    map
                },
            },
            VectorRecord {
                id: "2".to_string(),
                vector: vec![7.0, 8.0], // Wrong dimension
                metadata: {
                    let mut map = HashMap::new();
                    map.insert("title".to_string(), json!("Doc 2"));
                    map
                },
            },
        ];

        let result = validator.validate_batch(&records).unwrap();
        
        assert_eq!(result.valid_records.len(), 1);
        assert_eq!(result.duplicate_records.len(), 1);
        assert_eq!(result.invalid_records.len(), 1);
        assert_eq!(result.stats.total_processed, 3);
    }

    #[test]
    fn test_schema_validator() {
        let mut validator = SchemaValidator::new(false);
        validator.add_field("title".to_string(), FieldType::String);
        validator.add_field("score".to_string(), FieldType::Number);

        let record = VectorRecord {
            id: "1".to_string(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: {
                let mut map = HashMap::new();
                map.insert("title".to_string(), json!("Test"));
                map.insert("score".to_string(), json!(0.95));
                map
            },
        };

        assert!(validator.validate_schema(&record).is_ok());

        // Wrong type
        let mut bad_metadata = HashMap::new();
        bad_metadata.insert("title".to_string(), json!(123)); // Should be string
        bad_metadata.insert("score".to_string(), json!(0.95));
        
        let bad_record = VectorRecord {
            id: "2".to_string(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: bad_metadata,
        };

        assert!(validator.validate_schema(&bad_record).is_err());
    }
}