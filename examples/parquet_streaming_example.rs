//! Example demonstrating production-grade Parquet async streaming with RTDB
//!
//! This example shows how to use the enhanced Parquet streaming capabilities
//! for efficient migration and data processing.

use rtdb::migration::{
    parquet_streaming::{ParquetStreamConfig, ParquetStreamReader, ParquetStreamWriter},
    VectorRecord,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tokio::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!(" RTDB Parquet Streaming Example");
    
    // Create sample data
    let sample_data = create_sample_vector_data(10_000);
    println!(" Created {} sample vector records", sample_data.len());
    
    // Configure streaming with production settings
    let config = ParquetStreamConfig {
        batch_size: 1024,
        row_group_size: 100_000,
        compression: parquet::basic::Compression::ZSTD(parquet::basic::ZstdLevel::default()),
        dictionary_enabled: true,
        statistics_enabled: parquet::file::properties::EnabledStatistics::Chunk,
        buffer_size: 16,
        operation_timeout: tokio::time::Duration::from_secs(30),
        max_memory_usage: 256 * 1024 * 1024, // 256MB
    };
    
    let temp_file = "example_vectors.parquet";
    
    // Demonstrate writing with streaming
    println!("\n Writing vectors to Parquet with streaming...");
    let write_start = Instant::now();
    
    let mut writer = ParquetStreamWriter::new(Path::new(temp_file), config.clone()).await?;
    
    // Write in chunks to demonstrate streaming
    for chunk in sample_data.chunks(1000) {
        writer.write_records(chunk).await?;
    }
    
    let write_stats = writer.finalize().await?;
    let write_duration = write_start.elapsed();
    
    println!(" Write completed:");
    println!("    Rows written: {}", write_stats.rows_read);
    println!("    Duration: {:?}", write_duration);
    println!("    Throughput: {:.0} rows/sec", write_stats.rows_per_second);
    
    // Demonstrate reading with streaming
    println!("\n Reading vectors from Parquet with streaming...");
    let read_start = Instant::now();
    
    let mut reader = ParquetStreamReader::new(Path::new(temp_file), config).await?;
    let mut total_read = 0;
    let mut batch_count = 0;
    
    let mut stream = reader.stream_records();
    while let Some(batch_result) = stream.next().await {
        let batch = batch_result?;
        total_read += batch.len();
        batch_count += 1;
        
        // Process first few records as example
        if batch_count == 1 {
            println!("First batch sample:");
            for (i, record) in batch.iter().take(3).enumerate() {
                println!("     {}. ID: {}, Vector dim: {}, Metadata keys: {}", 
                    i + 1, record.id, record.vector.len(), record.metadata.len());
            }
        }
    }
    
    let read_stats = reader.get_stats();
    let read_duration = read_start.elapsed();
    
    println!(" Read completed:");
    println!("   Rows read: {}", total_read);
    println!("   Batches processed: {}", batch_count);
    println!("   Duration: {:?}", read_duration);
    println!("   Throughput: {:.0} rows/sec", read_stats.rows_per_second);
    
    // Demonstrate file validation
    println!("\n Validating Parquet file...");
    let file_info = rtdb::migration::parquet_streaming::utils::validate_parquet_file(
        Path::new(temp_file)
    ).await?;
    
    println!("File validation:");
    println!("   Rows: {}", file_info.num_rows);
    println!("   Row groups: {}", file_info.num_row_groups);
    println!("   File size: {:.2} MB", file_info.file_size as f64 / 1024.0 / 1024.0);
    println!("   Compression: {}", file_info.compression);
    println!("   Schema fields: {}", file_info.schema_fields);
    
    // Demonstrate memory usage estimation
    let memory_usage = rtdb::migration::parquet_streaming::utils::estimate_memory_usage(&sample_data[..100]);
    println!("   Memory usage (100 records): {:.2} KB", memory_usage as f64 / 1024.0);
    
    // Clean up
    if std::path::Path::new(temp_file).exists() {
        std::fs::remove_file(temp_file)?;
        println!("\n Cleaned up temporary file");
    }
    
    println!("\n Example completed successfully!");
    
    Ok(())
}

/// Create sample vector data for demonstration
fn create_sample_vector_data(count: usize) -> Vec<VectorRecord> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    (0..count)
        .map(|i| {
            let vector: Vec<f32> = (0..128)
                .map(|_| rng.gen_range(-1.0..1.0))
                .collect();
            
            let mut metadata = HashMap::new();
            metadata.insert("index".to_string(), serde_json::Value::Number(i.into()));
            metadata.insert("category".to_string(), serde_json::Value::String(
                format!("category_{}", i % 10)
            ));
            metadata.insert("score".to_string(), serde_json::Value::Number(
                serde_json::Number::from_f64(rng.gen_range(0.0..1.0)).unwrap()
            ));
            
            VectorRecord {
                id: format!("vec_{:06}", i),
                vector,
                metadata,
            }
        })
        .collect()
}