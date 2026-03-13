//! Comprehensive tests for production-grade Parquet streaming
//!
//! Tests cover:
//! - Basic streaming operations
//! - Error handling and recovery
//! - Performance characteristics
//! - Memory usage patterns
//! - Large file handling

#[cfg(test)]
mod tests {
    use super::super::parquet_streaming::*;
    use super::super::VectorRecord;
    use futures::StreamExt;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio::time::{Duration, Instant};

    /// Create test vector records
    fn create_test_records(count: usize, vector_dim: usize) -> Vec<VectorRecord> {
        (0..count)
            .map(|i| {
                let vector = (0..vector_dim).map(|j| (i * vector_dim + j) as f32).collect();
                let mut metadata = HashMap::new();
                metadata.insert("id".to_string(), serde_json::Value::Number(i.into()));
                metadata.insert("type".to_string(), serde_json::Value::String("test".to_string()));
                
                VectorRecord {
                    id: format!("test_{}", i),
                    vector,
                    metadata,
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn test_basic_streaming_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_basic.parquet");
        
        let test_records = create_test_records(1000, 128);
        let config = ParquetStreamConfig::default();
        
        // Write records
        let mut writer = ParquetStreamWriter::new(&file_path, config.clone()).await.unwrap();
        writer.write_records(&test_records).await.unwrap();
        let write_stats = writer.finalize().await.unwrap();
        
        assert_eq!(write_stats.rows_read, 1000);
        assert!(write_stats.rows_per_second > 0.0);
        
        // Read records back
        let mut reader = ParquetStreamReader::new(&file_path, config).await.unwrap();
        let mut all_records = Vec::new();
        
        let mut stream = reader.stream_records();
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.unwrap();
            all_records.extend(batch);
        }
        
        // Verify data integrity
        assert_eq!(all_records.len(), 1000);
        for (i, record) in all_records.iter().enumerate() {
            assert_eq!(record.id, format!("test_{}", i));
            assert_eq!(record.vector.len(), 128);
            assert_eq!(record.metadata.get("id").unwrap().as_u64().unwrap(), i as u64);
        }
        
        let read_stats = reader.get_stats();
        assert_eq!(read_stats.rows_read, 1000);
        assert!(read_stats.rows_per_second > 0.0);
    }

    #[tokio::test]
    async fn test_large_file_streaming() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_large.parquet");
        
        let record_count = 50_000;
        let test_records = create_test_records(record_count, 256);
        
        let config = ParquetStreamConfig {
            batch_size: 2048,
            row_group_size: 10_000,
            ..Default::default()
        };
        
        // Write large dataset
        let write_start = Instant::now();
        let mut writer = ParquetStreamWriter::new(&file_path, config.clone()).await.unwrap();
        
        // Write in chunks to test streaming behavior
        for chunk in test_records.chunks(5000) {
            writer.write_records(chunk).await.unwrap();
        }
        
        let write_stats = writer.finalize().await.unwrap();
        let write_duration = write_start.elapsed();
        
        assert_eq!(write_stats.rows_read, record_count as u64);
        println!("Large file write: {} rows in {:?} ({:.0} rows/sec)", 
                 write_stats.rows_read, write_duration, write_stats.rows_per_second);
        
        // Read back with streaming
        let read_start = Instant::now();
        let mut reader = ParquetStreamReader::new(&file_path, config).await.unwrap();
        let mut total_read = 0;
        let mut batch_count = 0;
        
        let mut stream = reader.stream_records();
        while let Some(batch_result) = stream.next().await {
            let batch = batch_result.unwrap();
            total_read += batch.len();
            batch_count += 1;
            
            // Verify some records from each batch
            if !batch.is_empty() {
                assert!(batch[0].vector.len() == 256);
                assert!(batch[0].id.starts_with("test_"));
            }
        }
        
        let read_duration = read_start.elapsed();
        let read_stats = reader.get_stats();
        
        assert_eq!(total_read, record_count);
        println!("Large file read: {} rows in {:?} ({:.0} rows/sec)", 
                 total_read, read_duration, read_stats.rows_per_second);
    }

    #[tokio::test]
    async fn test_different_compression_algorithms() {
        let temp_dir = TempDir::new().unwrap();
        let test_records = create_test_records(1000, 128);
        
        let compressions = vec![
            ("snappy", parquet::basic::Compression::SNAPPY),
            ("gzip", parquet::basic::Compression::GZIP(parquet::basic::GzipLevel::default())),
            ("zstd", parquet::basic::Compression::ZSTD(parquet::basic::ZstdLevel::default())),
            ("lz4", parquet::basic::Compression::LZ4),
        ];
        
        for (name, compression) in compressions {
            let file_path = temp_dir.path().join(format!("test_{}.parquet", name));
            
            let config = ParquetStreamConfig {
                compression,
                ..Default::default()
            };
            
            // Write with specific compression
            let mut writer = ParquetStreamWriter::new(&file_path, config.clone()).await.unwrap();
            writer.write_records(&test_records).await.unwrap();
            let write_stats = writer.finalize().await.unwrap();
            
            assert_eq!(write_stats.rows_read, 1000);
            
            // Verify file can be read back
            let mut reader = ParquetStreamReader::new(&file_path, config).await.unwrap();
            let mut t