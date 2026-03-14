//! Comprehensive example of Parquet migration functionality in RTDB
//!
//! This example demonstrates:
//! 1. Importing data from Parquet files (LanceDB compatibility)
//! 2. Exporting RTDB collections to Parquet format
//! 3. Advanced streaming operations with large files
//! 4. Performance optimization techniques

use rtdb::migration::{
    VectorRecord,
    formats::{create_reader, create_writer, DataFormat, FormatReader, FormatWriter},
    clients::{ParquetSourceClient, SourceClient},
    parquet_streaming::{ParquetStreamConfig, ParquetStreamReader, ParquetStreamWriter, utils},
    MigrationConfig, MigrationManager, SourceType, MigrationStrategy,
};
use rtdb::Result;
use std::path::Path;
use tokio::time::Duration;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::init();
    
    println!(" RTDB Parquet Migration Example");
    println!("==================================");
    
    // Example 1: Basic Parquet Import/Export
    basic_parquet_operations().await?;
    
    // Example 2: High-Performance Streaming
    streaming_operations().await?;
    
    // Example 3: LanceDB Migration Simulation
    lancedb_migration_example().await?;
    
    // Example 4: Production Migration Workflow
    production_migration_workflow().await?;
    
    println!("\nAll examples completed successfully!");
    Ok(())
}

/// Example 1: Basic Parquet import/export operations
async fn basic_parquet_operations() -> Result<()> {
    println!("\n Example 1: Basic Parquet Operations");
    println!("-------------------------------------");
    
    // Create sample data
    let sample_data = create_sample_data(1000, 128);
    let temp_file = "/tmp/rtdb_example.parquet";
    
    // Export to Parquet
    println!(" Exporting {} records to Parquet...", sample_data.len());
    {
        let mut writer = create_writer(Path::new(temp_file), Some(DataFormat::Parquet)).await?;
        writer.write_batch(&sample_data).await?;
        writer.finalize().await?;
    }
    
    // Check file size
    let metadata = std::fs::metadata(temp_file)?;
    println!(" Parquet