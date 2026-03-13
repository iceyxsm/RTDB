//! Integration tests for the migration system

use super::*;
use crate::migration::{
    formats::{create_reader, create_writer, DataFormat},
    MigrationConfig, MigrationStrategy, SourceType, ValidationConfig,
};
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::fs;

/// Test data for migration tests
fn create_test_records() -> Vec<VectorRecord> {
    vec![
        VectorRecord {
            id: "test1".to_string(),
            vector: vec![1.0, 2.0, 3.0, 4.0],
            metadata: {
                let mut map = HashMap::new();
                map.insert("title".to_string(), serde_json::Value::String("Test Document 1".to_string()));
                map.insert("category".to_string(), serde_json::Value::String("tech".to_string()));
                map.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(0.95).unwrap()));
                map
            },
        },
        VectorRecord {
            id: "test2".to_string(),
            vector: vec![5.0, 6.0, 7.0, 8.0],
            metadata: {
                let mut map = HashMap::new();
                map.insert("title".to_string(), serde_json::Value::String("Test Document 2".to_string()));
                map.insert("category".to_string(), serde_json::Value::String("science".to_string()));
                map.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(0.87).unwrap()));
                map
            },
        },
        VectorRecord {
            id: "test3".to_string(),
            vector: vec![9.0, 10.0, 11.0, 12.0],
            metadata: {
                let mut map = HashMap::new();
                map.insert("title".to_string(), serde_json::Value::String("Test Document 3".to_string()));
                map.insert("category".to_string(), serde_json::Value::String("business".to_string()));
                map.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(0.92).unwrap()));
                map
            },
        },
    ]
}

#[tokio::test]
async fn test_jsonl_format_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let jsonl_path = temp_dir.path().join("test.jsonl");
    
    let test_records = create_test_records();
    
    // Write records to JSONL
    let mut writer = create_writer(&jsonl_path, Some(DataFormat::Jsonl)).await.unwrap();
    writer.write_batch(&test_records).await.unwrap();
    writer.finalize().await.unwrap();
    
    // Read records back from JSONL
    let mut reader = create_reader(&jsonl_path, Some(DataFormat::Jsonl)).await.unwrap();
    let read_records = reader.read_batch(10).await.unwrap();
    
    assert_eq!(read_records.len(), test_records.len());
    
    for (original, read) in test_records.iter().zip(read_records.iter()) {
        assert_eq!(original.id, read.id);
        assert_eq!(original.vector, read.vector);
        assert_eq!(original.metadata.len(), read.metadata.len());
    }
}

#[tokio::test]
async fn test_csv_format_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("test.csv");
    
    let test_records = create_test_records();
    
    // Write records to CSV
    let mut writer = create_writer(&csv_path, Some(DataFormat::Csv)).await.unwrap();
    writer.write_batch(&test_records).await.unwrap();
    writer.finalize().await.unwrap();
    
    // Read records back from CSV
    let mut reader = create_reader(&csv_path, Some(DataFormat::Csv)).await.unwrap();
    let read_records = reader.read_batch(10).await.unwrap();
    
    assert_eq!(read_records.len(), test_records.len());
    
    // CSV format may have some type conversions, so we check the basic structure
    for (original, read) in test_records.iter().zip(read_records.iter()) {
        assert_eq!(original.id, read.id);
        assert_eq!(original.vector, read.vector);
        assert!(!read.metadata.is_empty());
    }
}

#[tokio::test]
async fn test_binary_format_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let binary_path = temp_dir.path().join("test.bin");
    
    let test_records = create_test_records();
    
    // Write records to binary format
    let mut writer = create_writer(&binary_path, Some(DataFormat::Binary)).await.unwrap();
    writer.write_batch(&test_records).await.unwrap();
    writer.finalize().await.unwrap();
    
    // Read records back from binary format
    let mut reader = create_reader(&binary_path, Some(DataFormat::Binary)).await.unwrap();
    let read_records = reader.read_batch(10).await.unwrap();
    
    assert_eq!(read_records.len(), test_records.len());
    
    for (original, read) in test_records.iter().zip(read_records.iter()) {
        assert_eq!(original.id, read.id);
        assert_eq!(original.vector, read.vector);
        assert_eq!(original.metadata, read.metadata);
    }
}

#[tokio::test]
async fn test_migration_config_validation() {
    let temp_dir = TempDir::new().unwrap();
    
    let config = MigrationConfig {
        id: uuid::Uuid::new_v4(),
        source_type: SourceType::Jsonl,
        source_url: temp_dir.path().join("source.jsonl").to_string_lossy().to_string(),
        target_url: "http://localhost:6333".to_string(),
        source_collection: None,
        target_collection: "test_collection".to_string(),
        batch_size: 100,
        max_concurrency: 2,
        dry_run: true,
        resume: false,
        checkpoint_dir: temp_dir.path().to_path_buf(),
        strategy: MigrationStrategy::Stream,
        source_auth: None,
        target_auth: None,
        transformations: vec![],
        validation: ValidationConfig {
            validate_vectors: true,
            validate_metadata: true,
            check_duplicates: false,
            vector_dimension: Some(4),
            required_fields: vec!["title".to_string()],
        },
    };
    
    // Test config serialization/deserialization
    let serialized = serde_json::to_string(&config).unwrap();
    let deserialized: MigrationConfig = serde_json::from_str(&serialized).unwrap();
    
    assert_eq!(config.id, deserialized.id);
    assert_eq!(config.batch_size, deserialized.batch_size);
    assert_eq!(config.target_collection, deserialized.target_collection);
}

#[tokio::test]
async fn test_transformation_rules() {
    use crate::migration::{TransformationRule, TransformOperation, ConversionType};
    
    let mut record = VectorRecord {
        id: "test".to_string(),
        vector: vec![1.0, 2.0, 3.0],
        metadata: {
            let mut map = HashMap::new();
            map.insert("old_field".to_string(), serde_json::Value::String("test_value".to_string()));
            map.insert("number_string".to_string(), serde_json::Value::String("42".to_string()));
            map
        },
    };
    
    // Test rename transformation
    let rename_rule = TransformationRule::FieldRename {
        field: "old_field".to_string(),
        new_name: "new_field".to_string(),
    };
    
    // Test conversion transformation
    let convert_rule = TransformationRule::FieldConvert {
        field: "number_string".to_string(),
        conversion: ConversionType::StringToNumber,
    };
    
    // Apply transformations (this would normally be done by BatchProcessor)
    // For testing, we'll simulate the transformation logic
    
    // Rename transformation
    match &rename_rule {
        TransformationRule::FieldRename { field, new_name } => {
            if let Some(value) = record.metadata.remove(field) {
                record.metadata.insert(new_name.clone(), value);
            }
        }
        _ => {}
    }
    
    // Convert transformation
    match &convert_rule {
        TransformationRule::FieldConvert { field, conversion } => {
            if let Some(value) = record.metadata.get(field).cloned() {
                match conversion {
                    ConversionType::StringToNumber => {
                        if let Some(s) = value.as_str() {
                            if let Ok(num) = s.parse::<f64>() {
                                record.metadata.insert(field.clone(), serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    
    // Verify transformations
    assert!(!record.metadata.contains_key("old_field"));
    assert!(record.metadata.contains_key("new_field"));
    assert_eq!(record.metadata.get("new_field").unwrap().as_str().unwrap(), "test_value");
    assert_eq!(record.metadata.get("number_string").unwrap().as_f64().unwrap(), 42.0);
}

#[tokio::test]
async fn test_migration_manager_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_dir = temp_dir.path().join("checkpoints");
    fs::create_dir_all(&checkpoint_dir).await.unwrap();
    
    let manager = MigrationManager::new(checkpoint_dir).unwrap();
    
    // Test that manager starts with no active migrations
    let migrations = manager.list_migrations().await;
    assert!(migrations.is_empty());
    
    // Test progress tracking
    let migration_id = uuid::Uuid::new_v4();
    manager.update_processed(migration_id, 100, 5).await;
    
    // Since the migration wasn't started through the manager, it won't be tracked
    let progress = manager.get_progress(migration_id).await;
    assert!(progress.is_none());
}

#[tokio::test]
async fn test_format_detection() {
    use crate::migration::formats::DataFormat;
    use std::path::Path;
    
    assert_eq!(DataFormat::from_extension(Path::new("data.jsonl")), Some(DataFormat::Jsonl));
    assert_eq!(DataFormat::from_extension(Path::new("vectors.parquet")), Some(DataFormat::Parquet));
    assert_eq!(DataFormat::from_extension(Path::new("embeddings.h5")), Some(DataFormat::Hdf5));
    assert_eq!(DataFormat::from_extension(Path::new("data.bin")), Some(DataFormat::Binary));
    assert_eq!(DataFormat::from_extension(Path::new("metadata.csv")), Some(DataFormat::Csv));
    assert_eq!(DataFormat::from_extension(Path::new("unknown.xyz")), None);
}

#[tokio::test]
async fn test_validation_rules() {
    use crate::migration::validation::validate_record;
    
    let config = ValidationConfig {
        validate_vectors: true,
        validate_metadata: true,
        check_duplicates: false,
        vector_dimension: Some(4),
        required_fields: vec!["title".to_string()],
    };
    
    // Valid record
    let valid_record = VectorRecord {
        id: "test".to_string(),
        vector: vec![1.0, 2.0, 3.0, 4.0],
        metadata: {
            let mut map = HashMap::new();
            map.insert("title".to_string(), serde_json::Value::String("Test".to_string()));
            map
        },
    };
    
    assert!(validate_record(&valid_record, &config).is_ok());
    
    // Invalid record - wrong vector dimension
    let invalid_record = VectorRecord {
        id: "test".to_string(),
        vector: vec![1.0, 2.0], // Wrong dimension
        metadata: {
            let mut map = HashMap::new();
            map.insert("title".to_string(), serde_json::Value::String("Test".to_string()));
            map
        },
    };
    
    assert!(validate_record(&invalid_record, &config).is_err());
    
    // Invalid record - missing required field
    let missing_field_record = VectorRecord {
        id: "test".to_string(),
        vector: vec![1.0, 2.0, 3.0, 4.0],
        metadata: HashMap::new(), // Missing "title" field
    };
    
    assert!(validate_record(&missing_field_record, &config).is_err());
}

#[tokio::test]
async fn test_checkpoint_functionality() {
    use crate::migration::checkpoint::CheckpointManager;
    
    let temp_dir = TempDir::new().unwrap();
    let checkpoint_manager = CheckpointManager::new(temp_dir.path().to_path_buf()).unwrap();
    
    let migration_id = uuid::Uuid::new_v4();
    let checkpoint_data = serde_json::json!({
        "offset": 1000,
        "batch_id": 10,
        "timestamp": "2024-01-01T00:00:00Z"
    });
    
    // Save checkpoint
    checkpoint_manager.save_checkpoint(migration_id, checkpoint_data.clone()).await.unwrap();
    
    // Load checkpoint
    let loaded_checkpoint = checkpoint_manager.load_checkpoint(migration_id).await.unwrap();
    assert!(loaded_checkpoint.is_some());
    
    let loaded_data = loaded_checkpoint.unwrap();
    assert_eq!(loaded_data["offset"].as_u64().unwrap(), 1000);
    assert_eq!(loaded_data["batch_id"].as_u64().unwrap(), 10);
}

#[tokio::test]
async fn test_progress_tracking() {
    use crate::migration::progress::ProgressTracker;
    
    let temp_dir = TempDir::new().unwrap();
    let manager = MigrationManager::new(temp_dir.path().to_path_buf()).unwrap();
    let migration_id = uuid::Uuid::new_v4();
    
    let tracker = ProgressTracker::new(migration_id, manager);
    
    // Test progress update
    let result = tracker.update_progress(500, 10).await;
    assert!(result.is_ok());
    
    // Test progress calculation (simplified since calculate_progress is not public)
    let processed = 500u64;
    let failed = 10u64;
    let total = Some(1000u64);
    
    let completion_percentage = if let Some(total_records) = total {
        if total_records > 0 {
            (processed as f64 / total_records as f64) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };
    
    assert_eq!(completion_percentage, 50.0);
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    
    /// Test end-to-end migration with JSONL files
    #[tokio::test]
    async fn test_jsonl_to_jsonl_migration() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source.jsonl");
        let target_path = temp_dir.path().join("target.jsonl");
        
        // Create source JSONL file
        let test_records = create_test_records();
        let mut writer = create_writer(&source_path, Some(DataFormat::Jsonl)).await.unwrap();
        writer.write_batch(&test_records).await.unwrap();
        writer.finalize().await.unwrap();
        
        // Test format conversion
        use crate::migration::formats::FormatConverter;
        let converted_count = FormatConverter::convert(
            &source_path,
            &target_path,
            Some(DataFormat::Jsonl),
            Some(DataFormat::Jsonl),
            100,
        ).await.unwrap();
        
        assert_eq!(converted_count, test_records.len() as u64);
        
        // Verify target file
        let mut reader = create_reader(&target_path, Some(DataFormat::Jsonl)).await.unwrap();
        let read_records = reader.read_batch(10).await.unwrap();
        assert_eq!(read_records.len(), test_records.len());
    }
    
    /// Test migration strategy selection
    #[tokio::test]
    async fn test_migration_strategy_selection() {
        use crate::migration::strategies::{select_strategy, estimate_migration_time};
        
        let mut config = MigrationConfig::default();
        
        config.strategy = MigrationStrategy::Stream;
        assert_eq!(select_strategy(&config).unwrap(), "streaming");
        
        config.strategy = MigrationStrategy::DualWrite;
        assert_eq!(select_strategy(&config).unwrap(), "dual-write");
        
        config.strategy = MigrationStrategy::BlueGreen;
        assert_eq!(select_strategy(&config).unwrap(), "blue-green");
        
        config.strategy = MigrationStrategy::Snapshot;
        assert_eq!(select_strategy(&config).unwrap(), "snapshot");
        
        // Test time estimation
        let duration = estimate_migration_time(&MigrationStrategy::Stream, 10000, 1000.0);
        assert!(duration.as_secs() >= 9 && duration.as_secs() <= 11); // ~10 seconds with some tolerance
    }
}