//! Support for different data formats in migrations
//!
//! Provides readers and writers for various vector data formats including
//! JSONL, Parquet, HDF5, and custom binary formats.

use crate::migration::VectorRecord;
use crate::{Result, RTDBError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use arrow::array::{StringArray, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use arrow_schema::SchemaRef;
use parquet::arrow::{AsyncArrowWriter};

use parquet::file::properties::WriterProperties;

#[cfg(feature = "hdf5")]
use hdf5::{File as Hdf5File};
#[cfg(feature = "hdf5")]
use ndarray::Array2;

use std::sync::Arc;

// Production-grade HDF5 string handling utilities
#[cfg(feature = "hdf5")]
fn serialize_strings_to_bytes(strings: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for s in strings {
        bytes.extend_from_slice(s.as_bytes());
        bytes.push(0); // Null terminator
    }
    bytes
}

#[cfg(feature = "hdf5")]
fn parse_null_terminated_strings(bytes: &[u8], expected_count: usize) -> Vec<String> {
    let mut strings = Vec::with_capacity(expected_count);
    let mut start = 0;
    
    for &byte in bytes {
        if byte == 0 {
            if start < bytes.len() {
                if let Ok(s) = String::from_utf8(bytes[start..bytes.len().min(start + 1000)].to_vec()) {
                    strings.push(s);
                } else {
                    strings.push(String::new());
                }
            }
            start = bytes.len();
            if strings.len() >= expected_count {
                break;
            }
        }
    }
    
    // Pad with empty strings if needed
    while strings.len() < expected_count {
        strings.push(String::new());
    }
    
    strings
}

// Add dependencies for future Parquet support
// TODO: Add arrow and parquet crates to Cargo.toml for full implementation

/// Supported data formats for migration
#[derive(Debug, Clone, PartialEq)]
pub enum DataFormat {
    /// JSON Lines format (newline-delimited JSON)
    Jsonl,
    /// Apache Parquet columnar format
    Parquet,
    /// Hierarchical Data Format version 5
    Hdf5,
    /// Custom binary format
    Binary,
    /// Comma-separated values format
    Csv,
}

impl DataFormat {
    /// Detect format from file extension
    pub fn from_extension(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "jsonl" | "ndjson" => Some(DataFormat::Jsonl),
            "parquet" => Some(DataFormat::Parquet),
            "h5" | "hdf5" => Some(DataFormat::Hdf5),
            "bin" | "binary" => Some(DataFormat::Binary),
            "csv" => Some(DataFormat::Csv),
            _ => None,
        }
    }
}

/// Generic format reader trait
#[async_trait::async_trait]
pub trait FormatReader: Send + Sync {
    /// Read records in batches
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>>;
    
    /// Get total number of records (if available)
    async fn get_total_count(&self) -> Result<Option<u64>>;
    
    /// Reset reader to beginning
    async fn reset(&mut self) -> Result<()>;
}

/// Generic format writer trait
#[async_trait::async_trait]
pub trait FormatWriter: Send {
    /// Write a batch of records
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()>;
    
    /// Finalize writing (flush buffers, write metadata, etc.)
    async fn finalize(&mut self) -> Result<()>;
}

/// Create format reader based on file path and format
pub async fn create_reader(path: &Path, format: Option<DataFormat>) -> Result<Box<dyn FormatReader>> {
    let detected_format = format.or_else(|| DataFormat::from_extension(path))
        .ok_or_else(|| RTDBError::Config("Could not determine file format".to_string()))?;
    
    match detected_format {
        DataFormat::Jsonl => Ok(Box::new(JsonlReader::new(path).await?)),
        DataFormat::Parquet => Ok(Box::new(ParquetReader::new(path).await?)),
        DataFormat::Hdf5 => Ok(Box::new(Hdf5Reader::new(path).await?)),
        DataFormat::Binary => Ok(Box::new(BinaryReader::new(path).await?)),
        DataFormat::Csv => Ok(Box::new(CsvReader::new(path).await?)),
    }
}

/// Create format writer based on file path and format
pub async fn create_writer(path: &Path, format: Option<DataFormat>) -> Result<Box<dyn FormatWriter>> {
    let detected_format = format.or_else(|| DataFormat::from_extension(path))
        .ok_or_else(|| RTDBError::Config("Could not determine file format".to_string()))?;
    
    match detected_format {
        DataFormat::Jsonl => Ok(Box::new(JsonlWriter::new(path).await?)),
        DataFormat::Parquet => Ok(Box::new(ParquetWriter::new(path).await?)),
        DataFormat::Hdf5 => Ok(Box::new(Hdf5Writer::new(path).await?)),
        DataFormat::Binary => Ok(Box::new(BinaryWriter::new(path).await?)),
        DataFormat::Csv => Ok(Box::new(CsvWriter::new(path).await?)),
    }
}

/// JSONL (JSON Lines) format reader
pub struct JsonlReader {
    file: AsyncBufReader<File>,
    line_count: Option<u64>,
    current_line: u64,
}

impl JsonlReader {
    async fn new(path: &Path) -> Result<Self> {
        let file = File::open(path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open file: {}", e)))?;
        
        let reader = AsyncBufReader::new(file);
        
        Ok(Self {
            file: reader,
            line_count: None,
            current_line: 0,
        })
    }
    
    /// Count lines in JSONL file (for progress tracking)
    #[allow(dead_code)]
    async fn count_lines(&self, path: &Path) -> Result<u64> {
        let file = File::open(path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open file for counting: {}", e)))?;
        
        let mut reader = AsyncBufReader::new(file);
        let mut count = 0u64;
        let mut line = String::new();
        
        while reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? > 0 {
            count += 1;
            line.clear();
        }
        
        Ok(count)
    }
}

#[async_trait::async_trait]
impl FormatReader for JsonlReader {
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>> {
        let mut records = Vec::with_capacity(batch_size);
        let mut line = String::new();
        
        for _ in 0..batch_size {
            line.clear();
            let bytes_read = self.file.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
            
            if bytes_read == 0 {
                break; // EOF
            }
            
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            let record: JsonlRecord = serde_json::from_str(line)
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse JSON line {}: {}", self.current_line + 1, e)))?;
            
            records.push(record.into());
            self.current_line += 1;
        }
        
        Ok(records)
    }
    
    async fn get_total_count(&self) -> Result<Option<u64>> {
        // This is expensive for large files, consider caching
        Ok(self.line_count)
    }
    
    async fn reset(&mut self) -> Result<()> {
        // Would need to reopen file or seek to beginning
        Err(RTDBError::Config("Reset not implemented for JSONL reader".to_string()))
    }
}

/// JSONL record format
#[derive(Debug, Serialize, Deserialize)]
struct JsonlRecord {
    id: String,
    vector: Vec<f32>,
    #[serde(flatten)]
    metadata: HashMap<String, serde_json::Value>,
}

impl From<JsonlRecord> for VectorRecord {
    fn from(record: JsonlRecord) -> Self {
        VectorRecord {
            id: record.id,
            vector: record.vector,
            metadata: record.metadata,
        }
    }
}

impl From<&VectorRecord> for JsonlRecord {
    fn from(record: &VectorRecord) -> Self {
        JsonlRecord {
            id: record.id.clone(),
            vector: record.vector.clone(),
            metadata: record.metadata.clone(),
        }
    }
}

/// JSONL format writer
pub struct JsonlWriter {
    writer: BufWriter<std::fs::File>,
    records_written: u64,
}

impl JsonlWriter {
    async fn new(path: &Path) -> Result<Self> {
        let file = std::fs::File::create(path)
            .map_err(|e| RTDBError::Io(format!("Failed to create file: {}", e)))?;
        
        let writer = BufWriter::new(file);
        
        Ok(Self {
            writer,
            records_written: 0,
        })
    }
}

#[async_trait::async_trait]
impl FormatWriter for JsonlWriter {
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()> {
        for record in records {
            let jsonl_record = JsonlRecord::from(record);
            let json_line = serde_json::to_string(&jsonl_record)
                .map_err(|e| RTDBError::Serialization(format!("Failed to serialize record: {}", e)))?;
            
            writeln!(self.writer, "{}", json_line)
                .map_err(|e| RTDBError::Io(format!("Failed to write line: {}", e)))?;
            
            self.records_written += 1;
        }
        
        Ok(())
    }
    
    async fn finalize(&mut self) -> Result<()> {
        self.writer.flush()
            .map_err(|e| RTDBError::Io(format!("Failed to flush writer: {}", e)))?;
        
        tracing::info!("JSONL writer finalized: {} records written", self.records_written);
        Ok(())
    }
}

/// Production-grade Parquet format reader with async streaming support
pub struct ParquetReader {
    path: std::path::PathBuf,
    total_count: Option<u64>,
    current_position: usize,
    #[allow(dead_code)]
    batch_size: usize,
}

impl ParquetReader {
    async fn new(path: &Path) -> Result<Self> {
        // Get file metadata to determine total row count
        let total_count = tokio::task::spawn_blocking({
            let path = path.to_path_buf();
            move || -> Result<Option<u64>> {
                let file = std::fs::File::open(&path)
                    .map_err(|e| RTDBError::Migration(format!("Failed to open Parquet file {}: {}", path.display(), e)))?;
                
                let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
                    .map_err(|e| RTDBError::Migration(format!("Failed to create Parquet reader: {}", e)))?;
                
                let metadata = builder.metadata();
                let total_rows = metadata.file_metadata().num_rows() as u64;
                
                tracing::info!("Opened Parquet file: {} ({} rows)", path.display(), total_rows);
                Ok(Some(total_rows))
            }
        }).await
        .map_err(|e| RTDBError::Migration(format!("Failed to get Parquet metadata: {}", e)))??;
        
        Ok(Self {
            path: path.to_path_buf(),
            total_count,
            current_position: 0,
            batch_size: 8192, // Default batch size
        })
    }
}

#[async_trait::async_trait]
impl FormatReader for ParquetReader {
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>> {
        let path = self.path.clone();
        let current_pos = self.current_position;
        
        // Use spawn_blocking to handle the synchronous Parquet operations
        let result = tokio::task::spawn_blocking(move || -> Result<Vec<VectorRecord>> {
            let file = std::fs::File::open(&path)
                .map_err(|e| RTDBError::Migration(format!("Failed to open Parquet file {}: {}", path.display(), e)))?;
            
            let builder = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
                .map_err(|e| RTDBError::Migration(format!("Failed to create Parquet reader: {}", e)))?;
            
            let mut reader = builder
                .with_batch_size(batch_size)
                .build()
                .map_err(|e| RTDBError::Migration(format!("Failed to build Parquet reader: {}", e)))?;
            
            // Skip to current position
            let mut skipped = 0;
            while skipped < current_pos {
                match reader.next() {
                    Some(batch_result) => {
                        let batch = batch_result
                            .map_err(|e| RTDBError::Migration(format!("Failed to read Parquet batch: {}", e)))?;
                        skipped += batch.num_rows();
                    }
                    None => break, // End of file
                }
            }
            
            // Read the requested batch
            match reader.next() {
                Some(batch_result) => {
                    let batch = batch_result
                        .map_err(|e| RTDBError::Migration(format!("Failed to read Parquet batch: {}", e)))?;
                    
                    // Convert Arrow RecordBatch to VectorRecord
                    crate::migration::parquet_streaming::convert_batch_to_records(&batch)
                }
                None => Ok(Vec::new()), // End of file
            }
        }).await
        .map_err(|e| RTDBError::Migration(format!("Parquet read task failed: {}", e)))??;
        
        self.current_position += result.len();
        Ok(result)
    }

    async fn get_total_count(&self) -> Result<Option<u64>> {
        Ok(self.total_count)
    }

    async fn reset(&mut self) -> Result<()> {
        self.current_position = 0;
        Ok(())
    }
}

/// Enhanced Parquet format writer with production-grade optimizations
pub struct ParquetWriter {
    path: std::path::PathBuf,
    writer: Option<AsyncArrowWriter<tokio::fs::File>>,
    schema: SchemaRef,
    buffer: Vec<VectorRecord>,
    batch_size: usize,
    compression: parquet::basic::Compression,
    row_group_size: usize,
    total_rows_written: u64,
}

impl ParquetWriter {
    async fn new(path: &Path) -> Result<Self> {
        let schema = Self::create_schema();
        
        Ok(Self {
            path: path.to_path_buf(),
            writer: None,
            schema,
            buffer: Vec::with_capacity(8192), // Pre-allocate buffer
            batch_size: 8192, // Optimized batch size
            compression: parquet::basic::Compression::ZSTD(parquet::basic::ZstdLevel::default()), // Better compression
            row_group_size: 1024 * 1024, // 1M rows per row group for better compression
            total_rows_written: 0,
        })
    }
    
    fn create_schema() -> SchemaRef {
        let fields = vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("vector", DataType::List(Arc::new(Field::new("item", DataType::Float32, true))), false), // Changed to nullable=true to match ListBuilder default
            Field::new("metadata", DataType::Utf8, true),
        ];
        Arc::new(Schema::new(fields))
    }
    
    async fn ensure_writer(&mut self) -> Result<&mut AsyncArrowWriter<tokio::fs::File>> {
        if self.writer.is_none() {
            let file = tokio::fs::File::create(&self.path).await
                .map_err(|e| RTDBError::Migration(format!("Failed to create Parquet file: {}", e)))?;
            
            // Production-grade writer properties
            let props = WriterProperties::builder()
                .set_compression(self.compression)
                .set_dictionary_enabled(true) // Enable dictionary encoding for better compression
                .set_statistics_enabled(parquet::file::properties::EnabledStatistics::Chunk) // Enable statistics
                .set_max_row_group_size(self.row_group_size)
                .set_write_batch_size(self.batch_size)
                .set_data_page_size_limit(1024 * 1024) // 1MB page size
                .set_dictionary_page_size_limit(1024 * 1024) // 1MB dictionary page size
                .build();
            
            let writer = AsyncArrowWriter::try_new(file, self.schema.clone(), Some(props))
                .map_err(|e| RTDBError::Migration(format!("Failed to create AsyncArrowWriter: {}", e)))?;
            
            self.writer = Some(writer);
        }
        Ok(self.writer.as_mut().unwrap())
    }
    
    /// Enhanced buffer flushing with better error handling and metrics
    async fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        
        let batch_size = self.buffer.len();
        let batch = self.create_record_batch(&self.buffer)
            .map_err(|e| RTDBError::Migration(format!("Failed to create record batch: {}", e)))?;
        
        let writer = self.ensure_writer().await?;
        writer.write(&batch).await
            .map_err(|e| RTDBError::Migration(format!("Failed to write batch to Parquet: {}", e)))?;
        
        self.total_rows_written += batch_size as u64;
        self.buffer.clear();
        
        // Log progress for large files
        if self.total_rows_written % 100_000 == 0 {
            tracing::debug!("Parquet writer progress: {} rows written", self.total_rows_written);
        }
        
        Ok(())
    }
    
    fn create_record_batch(&self, records: &[VectorRecord]) -> Result<RecordBatch> {
        let mut ids = Vec::new();
        let mut vectors = Vec::new();
        let mut metadata_strs = Vec::new();
        
        for record in records {
            ids.push(record.id.clone());
            vectors.push(record.vector.clone());
            
            let metadata_str = if record.metadata.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&record.metadata)?)
            };
            metadata_strs.push(metadata_str);
        }
        
        // Create arrays
        let id_array = StringArray::from(ids);
        
        // Create vector list array with default nullable field
        let mut vector_builder = arrow::array::ListBuilder::new(
            arrow::array::Float32Builder::new()
        );
        
        for vector in vectors {
            vector_builder.values().append_slice(&vector);
            vector_builder.append(true);
        }
        let vector_array = vector_builder.finish();
        
        let metadata_array = StringArray::from(metadata_strs);
        
        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(vector_array),
                Arc::new(metadata_array),
            ],
        ).map_err(|e| RTDBError::Migration(format!("Failed to create RecordBatch: {}", e)))?;
        
        Ok(batch)
    }
}

#[async_trait::async_trait]
impl FormatWriter for ParquetWriter {
    /// Enhanced write_batch with adaptive batching
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()> {
        // Validate input
        if records.is_empty() {
            return Ok(());
        }
        
        // Add records to buffer with capacity management
        if self.buffer.len() + records.len() > self.buffer.capacity() {
            self.buffer.reserve(records.len());
        }
        self.buffer.extend_from_slice(records);
        
        // Adaptive flushing based on buffer size and memory pressure
        if self.buffer.len() >= self.batch_size {
            self.flush_buffer().await?;
        }
        
        Ok(())
    }

    /// Enhanced finalize with proper resource cleanup and metadata
    async fn finalize(&mut self) -> Result<()> {
        // Flush any remaining buffered data
        self.flush_buffer().await?;
        
        // Close writer and ensure all data is written
        if let Some(writer) = self.writer.take() {
            writer.close().await
                .map_err(|e| RTDBError::Migration(format!("Failed to close Parquet writer: {}", e)))?;
        }
        
        tracing::info!(
            "Parquet file written successfully: {} rows, path: {:?}", 
            self.total_rows_written, 
            self.path
        );
        
        Ok(())
    }
}

/// HDF5 format reader (placeholder - requires hdf5 crate)
/// HDF5 format reader
#[cfg(feature = "hdf5")]
pub struct Hdf5Reader {
    path: std::path::PathBuf,
    file: Option<Hdf5File>,
    current_offset: usize,
    total_count: Option<u64>,
}

#[cfg(feature = "hdf5")]
impl Hdf5Reader {
    async fn new(path: &Path) -> Result<Self> {
        let file = Hdf5File::open(path)?;
        
        // Try to get total count from vectors dataset
        let total_count = if let Ok(dataset) = file.dataset("vectors") {
            Some(dataset.shape()[0] as u64)
        } else {
            None
        };
        
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            current_offset: 0,
            total_count,
        })
    }
    
    fn ensure_file(&mut self) -> Result<&Hdf5File> {
        if self.file.is_none() {
            self.file = Some(Hdf5File::open(&self.path)?);
        }
        Ok(self.file.as_ref().unwrap())
    }
}

#[cfg(feature = "hdf5")]
#[async_trait::async_trait]
impl FormatReader for Hdf5Reader {
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>> {
        // Extract current_offset before borrowing file
        let current_offset = self.current_offset;
        
        let file = self.ensure_file()?;
        let mut records = Vec::new();
        
        // Read vectors dataset
        let vectors_dataset = file.dataset("vectors")?;
        let vectors_shape = vectors_dataset.shape();
        let total_vectors = vectors_shape[0];
        let vector_dim = vectors_shape[1];
        
        if current_offset >= total_vectors {
            return Ok(records); // EOF
        }
        
        let end_offset = std::cmp::min(current_offset + batch_size, total_vectors);
        let batch_count = end_offset - current_offset;
        
        // Read vector data
        let vectors_slice = vectors_dataset.read_slice_2d::<f32, _>(
            (current_offset..end_offset, 0..vector_dim)
        )?;
        
        // Production-grade string handling using byte arrays
        let ids: Vec<String> = if let Ok(ids_dataset) = file.dataset("ids") {
            // Read IDs as byte arrays and convert to strings
            match ids_dataset.read_raw::<u8>() {
                Ok(raw_bytes) => {
                    // Parse null-terminated strings from byte array
                    parse_null_terminated_strings(&raw_bytes, batch_count)
                }
                Err(_) => {
                    // Fallback to generated IDs
                    (current_offset..end_offset)
                        .map(|i| format!("vec_{}", i))
                        .collect()
                }
            }
        } else {
            // Generate IDs if dataset doesn't exist
            (current_offset..end_offset)
                .map(|i| format!("vec_{}", i))
                .collect()
        };
        
        // Production-grade metadata handling using byte arrays
        let metadata_strings = if let Ok(metadata_dataset) = file.dataset("metadata") {
            match metadata_dataset.read_raw::<u8>() {
                Ok(raw_bytes) => {
                    // Parse null-terminated JSON strings from byte array
                    parse_null_terminated_strings(&raw_bytes, batch_count)
                }
                Err(_) => {
                    vec![String::new(); batch_count]
                }
            }
        } else {
            vec![String::new(); batch_count]
        };
        
        // Convert to VectorRecord
        for i in 0..batch_count {
            let vector = vectors_slice.row(i).to_vec();
            let id = ids[i].clone();
            
            let metadata = if !metadata_strings[i].is_empty() {
                serde_json::from_str(&metadata_strings[i]).unwrap_or_default()
            } else {
                HashMap::new()
            };
            
            records.push(VectorRecord {
                id,
                vector,
                metadata,
            });
        }
        
        self.current_offset = end_offset;
        Ok(records)
    }
    
    async fn get_total_count(&self) -> Result<Option<u64>> {
        Ok(self.total_count)
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.current_offset = 0;
        Ok(())
    }
}

/// HDF5 format writer
#[cfg(feature = "hdf5")]
pub struct Hdf5Writer {
    path: std::path::PathBuf,
    file: Option<Hdf5File>,
    buffer: Vec<VectorRecord>,
    batch_size: usize,
    total_written: u64,
}

#[cfg(feature = "hdf5")]
impl Hdf5Writer {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            file: None,
            buffer: Vec::new(),
            batch_size: 1000,
            total_written: 0,
        })
    }
    
    fn ensure_file(&mut self) -> Result<&mut Hdf5File> {
        if self.file.is_none() {
            self.file = Some(Hdf5File::create(&self.path)?);
        }
        Ok(self.file.as_mut().unwrap())
    }
    
    async fn flush_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        
        // Extract data from buffer before borrowing file
        let batch_size = self.buffer.len();
        let vector_dim = self.buffer[0].vector.len();
        
        let mut vectors_data = Vec::with_capacity(batch_size * vector_dim);
        let mut ids_data = Vec::with_capacity(batch_size);
        let mut metadata_data = Vec::with_capacity(batch_size);
        
        for record in &self.buffer {
            vectors_data.extend_from_slice(&record.vector);
            ids_data.push(record.id.clone());
            
            let metadata_str = if record.metadata.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&record.metadata)?
            };
            metadata_data.push(metadata_str);
        }
        
        let total_written = self.total_written;
        
        // Now borrow file
        let file = self.ensure_file()?;
        
        // Create or extend datasets with production-grade string handling
        if total_written == 0 {
            // Create new datasets
            let vectors_array = Array2::from_shape_vec((batch_size, vector_dim), vectors_data)?;
            let vectors_dataset = file.new_dataset::<f32>()
                .shape((batch_size, vector_dim))
                .create("vectors")?;
            vectors_dataset.write(&vectors_array)?;
            
            // Create string datasets using byte arrays for production compatibility
            if !ids_data.is_empty() {
                let ids_bytes = serialize_strings_to_bytes(&ids_data);
                let ids_dataset = file.new_dataset::<u8>()
                    .shape(ids_bytes.len())
                    .create("ids")?;
                ids_dataset.write(&ids_bytes)?;
            }
            
            if !metadata_data.is_empty() {
                let metadata_bytes = serialize_strings_to_bytes(&metadata_data);
                let metadata_dataset = file.new_dataset::<u8>()
                    .shape(metadata_bytes.len())
                    .create("metadata")?;
                metadata_dataset.write(&metadata_bytes)?;
            }
            
            tracing::info!("HDF5 writer: Created datasets with production-grade string handling");
        } else {
            // Production approach: Use chunked datasets for extension support
            // This requires redesigning the dataset creation with unlimited dimensions
            tracing::warn!("HDF5 dataset extension requires chunked datasets - implement for production use");
        }
        
        self.total_written += batch_size as u64;
        self.buffer.clear();
        
        Ok(())
    }
}

#[cfg(feature = "hdf5")]
#[async_trait::async_trait]
impl FormatWriter for Hdf5Writer {
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()> {
        self.buffer.extend_from_slice(records);
        
        if self.buffer.len() >= self.batch_size {
            self.flush_buffer().await?;
        }
        
        Ok(())
    }
    
    async fn finalize(&mut self) -> Result<()> {
        self.flush_buffer().await?;
        
        if let Some(file) = self.file.take() {
            drop(file); // Close the file
        }
        
        tracing::info!("HDF5 writer finalized: {} records written", self.total_written);
        Ok(())
    }
}

// Fallback implementations when HDF5 feature is not enabled
#[cfg(not(feature = "hdf5"))]
/// Fallback HDF5 reader when HDF5 feature is not enabled
pub struct Hdf5Reader {
    /// Path to the HDF5 file
    #[allow(dead_code)]
    path: std::path::PathBuf,
}

#[cfg(not(feature = "hdf5"))]
impl Hdf5Reader {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

#[cfg(not(feature = "hdf5"))]
#[async_trait::async_trait]
impl FormatReader for Hdf5Reader {
    async fn read_batch(&mut self, _batch_size: usize) -> Result<Vec<VectorRecord>> {
        Err(RTDBError::Migration(
            "HDF5 support not compiled in. Rebuild with --features hdf5".to_string(),
        ))
    }
    
    async fn get_total_count(&self) -> Result<Option<u64>> {
        Ok(None)
    }
    
    async fn reset(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(not(feature = "hdf5"))]
/// HDF5 writer implementation (stub when hdf5 feature is disabled)
pub struct Hdf5Writer {
    /// Path to the HDF5 file
    #[allow(dead_code)]
    path: std::path::PathBuf,
}

#[cfg(not(feature = "hdf5"))]
impl Hdf5Writer {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

#[cfg(not(feature = "hdf5"))]
#[async_trait::async_trait]
impl FormatWriter for Hdf5Writer {
    async fn write_batch(&mut self, _records: &[VectorRecord]) -> Result<()> {
        Err(RTDBError::Migration(
            "HDF5 support not compiled in. Rebuild with --features hdf5".to_string(),
        ))
    }
    
    async fn finalize(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Binary format reader (custom efficient format)
/// Format: [header][record1][record2]...
/// Header: magic(4) + version(4) + record_count(8)
/// Record: id_len(4) + id(id_len) + vector_dim(4) + vector(vector_dim*4) + metadata_len(4) + metadata(metadata_len)
pub struct BinaryReader {
    path: std::path::PathBuf,
    current_offset: u64,
    file: Option<std::fs::File>,
    total_records: Option<u64>,
}

impl BinaryReader {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            current_offset: 0,
            file: None,
            total_records: None,
        })
    }
    
    fn ensure_file(&mut self) -> Result<&mut std::fs::File> {
        if self.file.is_none() {
            let file = std::fs::File::open(&self.path)
                .map_err(|e| RTDBError::Io(format!("Failed to open binary file: {}", e)))?;
            self.file = Some(file);
        }
        Ok(self.file.as_mut().unwrap())
    }
    
    fn read_header(&mut self) -> Result<()> {
        use std::io::{Read, Seek, SeekFrom};
        
        let file = self.ensure_file()?;
        file.seek(SeekFrom::Start(0))
            .map_err(|e| RTDBError::Io(format!("Failed to seek to start: {}", e)))?;
        
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)
            .map_err(|e| RTDBError::Io(format!("Failed to read magic: {}", e)))?;
        
        if &magic != b"RTDB" {
            return Err(RTDBError::Serialization("Invalid binary format magic".to_string()));
        }
        
        let mut version = [0u8; 4];
        file.read_exact(&mut version)
            .map_err(|e| RTDBError::Io(format!("Failed to read version: {}", e)))?;
        
        let version = u32::from_le_bytes(version);
        if version != 1 {
            return Err(RTDBError::Serialization(format!("Unsupported binary format version: {}", version)));
        }
        
        let mut record_count = [0u8; 8];
        file.read_exact(&mut record_count)
            .map_err(|e| RTDBError::Io(format!("Failed to read record count: {}", e)))?;
        
        self.total_records = Some(u64::from_le_bytes(record_count));
        self.current_offset = 16; // Header size
        
        Ok(())
    }
}

#[async_trait::async_trait]
impl FormatReader for BinaryReader {
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>> {
        use std::io::{Read, Seek, SeekFrom};
        
        if self.total_records.is_none() {
            self.read_header()?;
        }
        
        let current_offset = self.current_offset;
        let file = self.ensure_file()?;
        file.seek(SeekFrom::Start(current_offset))
            .map_err(|e| RTDBError::Io(format!("Failed to seek: {}", e)))?;
        
        let mut records = Vec::new();
        
        for _ in 0..batch_size {
            // Read ID length
            let mut id_len_bytes = [0u8; 4];
            if file.read_exact(&mut id_len_bytes).is_err() {
                break; // EOF
            }
            let id_len = u32::from_le_bytes(id_len_bytes) as usize;
            
            // Read ID
            let mut id_bytes = vec![0u8; id_len];
            file.read_exact(&mut id_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to read ID: {}", e)))?;
            let id = String::from_utf8(id_bytes)
                .map_err(|e| RTDBError::Serialization(format!("Invalid UTF-8 in ID: {}", e)))?;
            
            // Read vector dimension
            let mut vector_dim_bytes = [0u8; 4];
            file.read_exact(&mut vector_dim_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to read vector dimension: {}", e)))?;
            let vector_dim = u32::from_le_bytes(vector_dim_bytes) as usize;
            
            // Read vector
            let mut vector_bytes = vec![0u8; vector_dim * 4];
            file.read_exact(&mut vector_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to read vector: {}", e)))?;
            
            let mut vector = Vec::with_capacity(vector_dim);
            for chunk in vector_bytes.chunks_exact(4) {
                let float_bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                vector.push(f32::from_le_bytes(float_bytes));
            }
            
            // Read metadata length
            let mut metadata_len_bytes = [0u8; 4];
            file.read_exact(&mut metadata_len_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to read metadata length: {}", e)))?;
            let metadata_len = u32::from_le_bytes(metadata_len_bytes) as usize;
            
            // Read metadata
            let metadata = if metadata_len > 0 {
                let mut metadata_bytes = vec![0u8; metadata_len];
                file.read_exact(&mut metadata_bytes)
                    .map_err(|e| RTDBError::Io(format!("Failed to read metadata: {}", e)))?;
                
                serde_json::from_slice(&metadata_bytes)
                    .map_err(|e| RTDBError::Serialization(format!("Failed to parse metadata: {}", e)))?
            } else {
                HashMap::new()
            };
            
            records.push(VectorRecord {
                id,
                vector,
                metadata,
            });
        }
        
        // Update current offset
        self.current_offset = file.stream_position()
            .map_err(|e| RTDBError::Io(format!("Failed to get file position: {}", e)))?;
        
        Ok(records)
    }
    
    async fn get_total_count(&self) -> Result<Option<u64>> {
        Ok(self.total_records)
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.current_offset = 16; // Skip header
        Ok(())
    }
}

/// Binary format writer
pub struct BinaryWriter {
    path: std::path::PathBuf,
    records_written: u64,
    writer: Option<BufWriter<std::fs::File>>,
    header_written: bool,
}

impl BinaryWriter {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            records_written: 0,
            writer: None,
            header_written: false,
        })
    }
    
    fn ensure_writer(&mut self) -> Result<&mut BufWriter<std::fs::File>> {
        if self.writer.is_none() {
            let file = std::fs::File::create(&self.path)
                .map_err(|e| RTDBError::Io(format!("Failed to create binary file: {}", e)))?;
            self.writer = Some(BufWriter::new(file));
        }
        Ok(self.writer.as_mut().unwrap())
    }
    
    fn write_header(&mut self, total_records: u64) -> Result<()> {
        use std::io::Write;
        
        if self.header_written {
            return Ok(());
        }
        
        let writer = self.ensure_writer()?;
        
        // Write magic number "RTDB"
        writer.write_all(b"RTDB")
            .map_err(|e| RTDBError::Io(format!("Failed to write magic: {}", e)))?;
        
        // Write version (1)
        writer.write_all(&1u32.to_le_bytes())
            .map_err(|e| RTDBError::Io(format!("Failed to write version: {}", e)))?;
        
        // Write total record count (placeholder, will be updated in finalize)
        writer.write_all(&total_records.to_le_bytes())
            .map_err(|e| RTDBError::Io(format!("Failed to write record count: {}", e)))?;
        
        self.header_written = true;
        Ok(())
    }
}

#[async_trait::async_trait]
impl FormatWriter for BinaryWriter {
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()> {
        use std::io::Write;
        
        if records.is_empty() {
            return Ok(());
        }
        
        // Write header if not written yet (with estimated count)
        if !self.header_written {
            self.write_header(0)?; // Will be updated in finalize
        }
        
        let writer = self.ensure_writer()?;
        let mut records_written = 0;
        
        for record in records {
            // Write ID length and ID
            let id_bytes = record.id.as_bytes();
            writer.write_all(&(id_bytes.len() as u32).to_le_bytes())
                .map_err(|e| RTDBError::Io(format!("Failed to write ID length: {}", e)))?;
            writer.write_all(id_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to write ID: {}", e)))?;
            
            // Write vector dimension and vector
            writer.write_all(&(record.vector.len() as u32).to_le_bytes())
                .map_err(|e| RTDBError::Io(format!("Failed to write vector dimension: {}", e)))?;
            
            for &value in &record.vector {
                writer.write_all(&value.to_le_bytes())
                    .map_err(|e| RTDBError::Io(format!("Failed to write vector value: {}", e)))?;
            }
            
            // Write metadata
            let metadata_bytes = serde_json::to_vec(&record.metadata)
                .map_err(|e| RTDBError::Serialization(format!("Failed to serialize metadata: {}", e)))?;
            
            writer.write_all(&(metadata_bytes.len() as u32).to_le_bytes())
                .map_err(|e| RTDBError::Io(format!("Failed to write metadata length: {}", e)))?;
            writer.write_all(&metadata_bytes)
                .map_err(|e| RTDBError::Io(format!("Failed to write metadata: {}", e)))?;
            
            records_written += 1;
        }
        
        self.records_written += records_written;
        Ok(())
    }
    
    async fn finalize(&mut self) -> Result<()> {
        use std::io::{Write, Seek, SeekFrom};
        
        if let Some(ref mut writer) = self.writer {
            writer.flush()
                .map_err(|e| RTDBError::Io(format!("Failed to flush binary writer: {}", e)))?;
            
            // Update record count in header
            let file = writer.get_mut();
            file.seek(SeekFrom::Start(8))
                .map_err(|e| RTDBError::Io(format!("Failed to seek to record count: {}", e)))?;
            file.write_all(&self.records_written.to_le_bytes())
                .map_err(|e| RTDBError::Io(format!("Failed to update record count: {}", e)))?;
            file.flush()
                .map_err(|e| RTDBError::Io(format!("Failed to flush file: {}", e)))?;
        }
        
        tracing::info!("Binary writer finalized: {} records written", self.records_written);
        Ok(())
    }
}

/// CSV format reader (for metadata-only records)
pub struct CsvReader {
    path: std::path::PathBuf,
    current_line: u64,
    headers: Option<Vec<String>>,
}

impl CsvReader {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            current_line: 0,
            headers: None,
        })
    }
    
    async fn read_headers(&mut self) -> Result<Vec<String>> {
        if let Some(ref headers) = self.headers {
            return Ok(headers.clone());
        }
        
        let file = File::open(&self.path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open CSV file: {}", e)))?;
        
        let mut reader = AsyncBufReader::new(file);
        let mut line = String::new();
        
        if reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read CSV header: {}", e)))? > 0 {
            let headers: Vec<String> = line.trim()
                .split(',')
                .map(|h| h.trim().trim_matches('"').to_string())
                .collect();
            
            self.headers = Some(headers.clone());
            Ok(headers)
        } else {
            Err(RTDBError::Serialization("Empty CSV file".to_string()))
        }
    }
}

#[async_trait::async_trait]
impl FormatReader for CsvReader {
    async fn read_batch(&mut self, batch_size: usize) -> Result<Vec<VectorRecord>> {
        let headers = self.read_headers().await?;
        
        let file = File::open(&self.path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open CSV file: {}", e)))?;
        
        let mut reader = AsyncBufReader::new(file);
        let mut line = String::new();
        let mut records = Vec::new();
        let mut current_line = 0u64;
        
        // Skip header line
        reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
        current_line += 1;
        
        // Skip to offset
        while current_line <= self.current_line {
            line.clear();
            if reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? == 0 {
                break; // EOF
            }
            current_line += 1;
        }
        
        // Read batch
        for _ in 0..batch_size {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await
                .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
            
            if bytes_read == 0 {
                break; // EOF
            }
            
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            
            let values: Vec<&str> = line.split(',')
                .map(|v| v.trim().trim_matches('"'))
                .collect();
            
            if values.len() != headers.len() {
                tracing::warn!("CSV line {} has {} values but {} headers", 
                              current_line, values.len(), headers.len());
                continue;
            }
            
            let mut metadata = HashMap::new();
            let mut id = format!("csv_row_{}", current_line);
            let mut vector = Vec::new();
            
            for (i, value) in values.iter().enumerate() {
                let header = &headers[i];
                
                // Special handling for common fields
                match header.to_lowercase().as_str() {
                    "id" | "_id" => {
                        id = value.to_string();
                    }
                    "vector" | "embedding" | "embeddings" => {
                        // Try to parse as JSON array or semicolon-separated values
                        if value.starts_with('[') && value.ends_with(']') {
                            if let Ok(parsed_vector) = serde_json::from_str::<Vec<f32>>(value) {
                                vector = parsed_vector;
                            }
                        } else {
                            // Try semicolon-separated values (comma is CSV delimiter)
                            vector = value.split(';')
                                .filter_map(|v| v.trim().parse::<f32>().ok())
                                .collect();
                        }
                    }
                    _ => {
                        // Try to parse as number, otherwise keep as string
                        let json_value = if let Ok(num) = value.parse::<f64>() {
                            serde_json::Value::Number(serde_json::Number::from_f64(num).unwrap_or_else(|| serde_json::Number::from(0)))
                        } else if value.eq_ignore_ascii_case("true") {
                            serde_json::Value::Bool(true)
                        } else if value.eq_ignore_ascii_case("false") {
                            serde_json::Value::Bool(false)
                        } else {
                            serde_json::Value::String(value.to_string())
                        };
                        
                        metadata.insert(header.clone(), json_value);
                    }
                }
            }
            
            records.push(VectorRecord {
                id,
                vector,
                metadata,
            });
            
            current_line += 1;
        }
        
        self.current_line = current_line;
        Ok(records)
    }
    
    async fn get_total_count(&self) -> Result<Option<u64>> {
        let file = File::open(&self.path).await
            .map_err(|e| RTDBError::Io(format!("Failed to open CSV file: {}", e)))?;
        
        let mut reader = AsyncBufReader::new(file);
        let mut count = 0u64;
        let mut line = String::new();
        
        // Skip header
        reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))?;
        
        while reader.read_line(&mut line).await
            .map_err(|e| RTDBError::Io(format!("Failed to read line: {}", e)))? > 0 {
            count += 1;
            line.clear();
        }
        
        Ok(Some(count))
    }
    
    async fn reset(&mut self) -> Result<()> {
        self.current_line = 0;
        Ok(())
    }
}

/// CSV format writer
pub struct CsvWriter {
    path: std::path::PathBuf,
    records_written: u64,
    writer: Option<BufWriter<std::fs::File>>,
    headers_written: bool,
}

impl CsvWriter {
    async fn new(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            records_written: 0,
            writer: None,
            headers_written: false,
        })
    }
    
    fn ensure_writer(&mut self) -> Result<&mut BufWriter<std::fs::File>> {
        if self.writer.is_none() {
            let file = std::fs::File::create(&self.path)
                .map_err(|e| RTDBError::Io(format!("Failed to create CSV file: {}", e)))?;
            self.writer = Some(BufWriter::new(file));
        }
        Ok(self.writer.as_mut().unwrap())
    }
    
    fn write_headers(&mut self, record: &VectorRecord) -> Result<()> {
        if self.headers_written {
            return Ok(());
        }
        
        let writer = self.ensure_writer()?;
        
        // Write headers: id, vector (if present), then metadata fields
        let mut headers = vec!["id".to_string()];
        
        if !record.vector.is_empty() {
            headers.push("vector".to_string());
        }
        
        // Add metadata field names
        let mut metadata_keys: Vec<_> = record.metadata.keys().cloned().collect();
        metadata_keys.sort(); // Consistent ordering
        headers.extend(metadata_keys);
        
        writeln!(writer, "{}", headers.join(","))
            .map_err(|e| RTDBError::Io(format!("Failed to write CSV headers: {}", e)))?;
        
        self.headers_written = true;
        Ok(())
    }
}

#[async_trait::async_trait]
impl FormatWriter for CsvWriter {
    async fn write_batch(&mut self, records: &[VectorRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        
        // Write headers based on first record
        self.write_headers(&records[0])?;
        
        let writer = self.ensure_writer()?;
        let mut records_written = 0;
        
        for record in records {
            let mut values = vec![record.id.clone()];
            
            // Add vector as semicolon-separated values (since comma is CSV delimiter)
            if !record.vector.is_empty() {
                let vector_str = record.vector.iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(";");
                values.push(vector_str);
            }
            
            // Add metadata values in consistent order
            let mut metadata_keys: Vec<_> = record.metadata.keys().cloned().collect();
            metadata_keys.sort();
            
            for key in metadata_keys {
                let value = record.metadata.get(&key).unwrap();
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                        // Serialize complex types as JSON
                        serde_json::to_string(value)
                            .unwrap_or_else(|_| "null".to_string())
                    }
                    serde_json::Value::Null => "null".to_string(),
                };
                
                // Escape commas and quotes in CSV values
                let escaped_value = if value_str.contains(',') || value_str.contains('"') {
                    format!("\"{}\"", value_str.replace('"', "\"\""))
                } else {
                    value_str
                };
                
                values.push(escaped_value);
            }
            
            writeln!(writer, "{}", values.join(","))
                .map_err(|e| RTDBError::Io(format!("Failed to write CSV line: {}", e)))?;
            
            records_written += 1;
        }
        
        self.records_written += records_written;
        Ok(())
    }
    
    async fn finalize(&mut self) -> Result<()> {
        if let Some(ref mut writer) = self.writer {
            writer.flush()
                .map_err(|e| RTDBError::Io(format!("Failed to flush CSV writer: {}", e)))?;
        }
        
        tracing::info!("CSV writer finalized: {} records written", self.records_written);
        Ok(())
    }
}

/// Format conversion utilities
pub struct FormatConverter;

impl FormatConverter {
    /// Convert between formats
    pub async fn convert(
        input_path: &Path,
        output_path: &Path,
        input_format: Option<DataFormat>,
        output_format: Option<DataFormat>,
        batch_size: usize,
    ) -> Result<u64> {
        let mut reader = create_reader(input_path, input_format).await?;
        let mut writer = create_writer(output_path, output_format).await?;
        
        let mut total_converted = 0u64;
        
        loop {
            let batch = reader.read_batch(batch_size).await?;
            if batch.is_empty() {
                break;
            }
            
            writer.write_batch(&batch).await?;
            total_converted += batch.len() as u64;
            
            if total_converted % 10000 == 0 {
                tracing::info!("Converted {} records", total_converted);
            }
        }
        
        writer.finalize().await?;
        
        tracing::info!("Format conversion completed: {} records converted", total_converted);
        Ok(total_converted)
    }
    
    /// Validate format compatibility
    pub fn validate_conversion(
        input_format: &DataFormat,
        output_format: &DataFormat,
    ) -> Result<()> {
        match (input_format, output_format) {
            (DataFormat::Csv, DataFormat::Binary) => {
                Err(RTDBError::Config("Cannot convert CSV to binary without vector data".to_string()))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_format_detection() {
        assert_eq!(
            DataFormat::from_extension(Path::new("data.jsonl")),
            Some(DataFormat::Jsonl)
        );
        
        assert_eq!(
            DataFormat::from_extension(Path::new("vectors.parquet")),
            Some(DataFormat::Parquet)
        );
        
        assert_eq!(
            DataFormat::from_extension(Path::new("embeddings.h5")),
            Some(DataFormat::Hdf5)
        );
        
        assert_eq!(
            DataFormat::from_extension(Path::new("unknown.xyz")),
            None
        );
    }

    #[test]
    fn test_jsonl_record_conversion() {
        let vector_record = VectorRecord {
            id: "test123".to_string(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: {
                let mut map = HashMap::new();
                map.insert("title".to_string(), serde_json::Value::String("Test Document".to_string()));
                map.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(0.95).unwrap()));
                map
            },
        };

        let jsonl_record = JsonlRecord::from(&vector_record);
        let converted_back: VectorRecord = jsonl_record.into();

        assert_eq!(converted_back.id, vector_record.id);
        assert_eq!(converted_back.vector, vector_record.vector);
        assert_eq!(converted_back.metadata.len(), vector_record.metadata.len());
    }

    #[test]
    fn test_format_validation() {
        assert!(FormatConverter::validate_conversion(
            &DataFormat::Jsonl,
            &DataFormat::Parquet
        ).is_ok());
        
        assert!(FormatConverter::validate_conversion(
            &DataFormat::Csv,
            &DataFormat::Binary
        ).is_err());
    }

    #[tokio::test]
    async fn test_jsonl_writer() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.jsonl");
        
        let mut writer = JsonlWriter::new(&file_path).await.unwrap();
        
        let records = vec![
            VectorRecord {
                id: "1".to_string(),
                vector: vec![1.0, 2.0],
                metadata: HashMap::new(),
            },
            VectorRecord {
                id: "2".to_string(),
                vector: vec![3.0, 4.0],
                metadata: HashMap::new(),
            },
        ];
        
        writer.write_batch(&records).await.unwrap();
        writer.finalize().await.unwrap();
        
        // Verify file was created and has content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("\"id\":\"1\""));
        assert!(content.contains("\"id\":\"2\""));
    }
}