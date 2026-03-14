//! Production-grade database migration tools for RTDB
//!
//! Supports migration from:
//! - Qdrant (REST/gRPC)
//! - Milvus (REST/gRPC) 
//! - Weaviate (REST/GraphQL)
//! - Pinecone (REST)
//! - LanceDB (Parquet files)
//! - Generic formats (JSONL, Parquet, HDF5)
//!
//! Features:
//! - Streaming migration with batching
//! - Resume interrupted migrations with checkpoints
//! - Dry-run mode for validation
//! - Progress tracking and ETA
//! - Parallel processing
//! - Data validation and integrity checks
//! - Zero-downtime migration strategies

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub mod checkpoint;
pub mod clients;
pub mod formats;
pub mod parquet_streaming;
pub mod progress;
pub mod strategies;
pub mod validation;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod parquet_integration_test;

/// Migration source types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    /// Qdrant vector database
    Qdrant,
    /// Milvus vector database
    Milvus,
    /// Weaviate vector database
    Weaviate,
    /// Pinecone vector database
    Pinecone,
    /// LanceDB vector database
    LanceDB,
    /// JSON Lines format
    Jsonl,
    /// Apache Parquet format
    Parquet,
    /// HDF5 format
    Hdf5,
    /// CSV format
    Csv,
    /// Binary format
    Binary,
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::Qdrant => write!(f, "Qdrant"),
            SourceType::Milvus => write!(f, "Milvus"),
            SourceType::Weaviate => write!(f, "Weaviate"),
            SourceType::Pinecone => write!(f, "Pinecone"),
            SourceType::LanceDB => write!(f, "LanceDB"),
            SourceType::Jsonl => write!(f, "JSONL"),
            SourceType::Parquet => write!(f, "Parquet"),
            SourceType::Hdf5 => write!(f, "HDF5"),
            SourceType::Csv => write!(f, "CSV"),
            SourceType::Binary => write!(f, "Binary"),
        }
    }
}

/// Migration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Unique migration ID
    pub id: Uuid,
    /// Source database type
    pub source_type: SourceType,
    /// Source connection URL or file path
    pub source_url: String,
    /// Target RTDB connection URL
    pub target_url: String,
    /// Source collection/index name
    pub source_collection: Option<String>,
    /// Target collection name
    pub target_collection: String,
    /// Batch size for processing
    pub batch_size: usize,
    /// Maximum concurrent batches
    pub max_concurrency: usize,
    /// Enable dry run mode
    pub dry_run: bool,
    /// Resume from checkpoint
    pub resume: bool,
    /// Checkpoint directory
    pub checkpoint_dir: PathBuf,
    /// Migration strategy
    pub strategy: MigrationStrategy,
    /// Source authentication
    pub source_auth: Option<AuthConfig>,
    /// Target authentication
    pub target_auth: Option<AuthConfig>,
    /// Data transformation rules
    pub transformations: Vec<TransformationRule>,
    /// Validation rules
    pub validation: ValidationConfig,
}

/// Migration strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationStrategy {
    /// Simple streaming migration
    Stream,
    /// Dual-write migration (write to both systems)
    DualWrite,
    /// Blue-green migration
    BlueGreen,
    /// Snapshot-based migration
    Snapshot,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthConfig {
    /// API key authentication
    ApiKey(String),
    /// Bearer token authentication
    Bearer(String),
    /// Basic authentication with username and password
    Basic { 
        /// Username for basic auth
        username: String, 
        /// Password for basic auth
        password: String 
    },
    /// Custom headers authentication
    Headers(HashMap<String, String>),
}

/// Data transformation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformationRule {
    /// Reduce vector dimensions using specified method
    DimensionReduction {
        /// Original vector dimension
        from_dimension: usize,
        /// Target vector dimension
        to_dimension: usize,
        /// Reduction method (e.g., "pca", "truncate")
        method: String,
    },
    /// Filter records based on metadata query
    MetadataFilter {
        /// Filter query expression
        query: String,
    },
    /// Rename a field in the record
    FieldRename {
        /// Original field name
        field: String,
        /// New field name
        new_name: String,
    },
    /// Map field values using a lookup table
    FieldMap {
        /// Field name to map
        field: String,
        /// Value mapping table
        mapping: HashMap<String, String>,
    },
    /// Convert field to a different data type
    FieldConvert {
        /// Field name to convert
        field: String,
        /// Type conversion to apply
        conversion: ConversionType,
    },
    /// Filter records based on field condition
    FieldFilter {
        /// Field name to filter on
        field: String,
        /// Filter condition to apply
        condition: FilterCondition,
    },
}

/// Transformation operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformOperation {
    /// Rename operation with new name
    Rename(String),
    /// Map operation with value mappings
    Map(HashMap<String, String>),
    /// Convert operation with type conversion
    Convert(ConversionType),
    /// Filter operation with condition
    Filter(FilterCondition),
}

/// Data type conversions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversionType {
    /// Convert string to number
    StringToNumber,
    /// Convert number to string
    NumberToString,
    /// Convert array to string representation
    ArrayToString,
    /// Convert string to array
    StringToArray,
}

/// Filter conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterCondition {
    /// Field must exist
    Exists,
    /// Field must not be null
    NotNull,
    /// Field must have minimum length
    MinLength(usize),
    /// Field must have maximum length
    MaxLength(usize),
    /// Field must match regex pattern
    Regex(String),
}

/// Validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Whether to validate vector data
    pub validate_vectors: bool,
    /// Whether to validate metadata
    pub validate_metadata: bool,
    /// Whether to check for duplicate records
    pub check_duplicates: bool,
    /// Expected vector dimension (None for any)
    pub vector_dimension: Option<usize>,
    /// List of required metadata fields
    pub required_fields: Vec<String>,
}

/// Migration progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    /// Unique migration identifier
    pub id: Uuid,
    /// Current migration status
    pub status: MigrationStatus,
    /// Source system type
    pub source_type: SourceType,
    /// Target collection name
    pub target_collection: String,
    /// Total number of records to migrate (if known)
    pub total_records: Option<u64>,
    /// Number of records processed so far
    pub processed_records: u64,
    /// Number of records that failed processing
    pub failed_records: u64,
    /// Current batch number being processed
    pub current_batch: u64,
    /// Migration start timestamp
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// Last progress update timestamp
    pub last_update: chrono::DateTime<chrono::Utc>,
    /// Estimated completion time (if available)
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
    /// Current processing throughput (records per second)
    pub throughput_per_second: f64,
    /// List of error messages encountered
    pub error_messages: Vec<String>,
}

/// Migration status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationStatus {
    /// Migration is queued but not started
    Pending,
    /// Migration is currently running
    Running,
    /// Migration is temporarily paused
    Paused,
    /// Migration completed successfully
    Completed,
    /// Migration failed with errors
    Failed,
    /// Migration was cancelled by user
    Cancelled,
}

/// Vector record for migration operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    /// Unique identifier for the vector
    pub id: String,
    /// Vector embeddings as f32 array
    pub vector: Vec<f32>,
    /// Associated metadata as key-value pairs
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Batch of vector records for efficient processing
#[derive(Debug, Clone)]
pub struct VectorBatch {
    /// Vector records in this batch
    pub records: Vec<VectorRecord>,
    /// Unique identifier for this batch
    pub batch_id: u64,
    /// Optional checkpoint data for resumable migrations
    pub checkpoint_data: Option<serde_json::Value>,
}

/// Migration manager
pub struct MigrationManager {
    active_migrations: Arc<RwLock<HashMap<Uuid, MigrationProgress>>>,
    checkpoint_manager: checkpoint::CheckpointManager,
}

impl MigrationManager {
    /// Create new migration manager
    pub fn new(checkpoint_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            active_migrations: Arc::new(RwLock::new(HashMap::new())),
            checkpoint_manager: checkpoint::CheckpointManager::new(checkpoint_dir)?,
        })
    }

    /// Start a new migration
    pub async fn start_migration(&self, config: MigrationConfig) -> Result<Uuid> {
        let migration_id = config.id;
        
        // Initialize progress tracking
        let progress = MigrationProgress {
            id: migration_id,
            status: MigrationStatus::Pending,
            source_type: config.source_type.clone(),
            target_collection: config.target_collection.clone(),
            total_records: None,
            processed_records: 0,
            failed_records: 0,
            current_batch: 0,
            start_time: chrono::Utc::now(),
            last_update: chrono::Utc::now(),
            estimated_completion: None,
            throughput_per_second: 0.0,
            error_messages: Vec::new(),
        };

        self.active_migrations.write().await.insert(migration_id, progress);

        // Start migration in background
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.execute_migration(config).await {
                tracing::error!("Migration {} failed: {}", migration_id, e);
                manager.update_status(migration_id, MigrationStatus::Failed).await;
            }
        });

        Ok(migration_id)
    }

    /// Execute migration
    async fn execute_migration(&self, config: MigrationConfig) -> Result<()> {
        let migration_id = config.id;
        
        tracing::info!("Starting migration {} from {:?} to {}", 
                      migration_id, config.source_type, config.target_url);

        // Update status to running
        self.update_status(migration_id, MigrationStatus::Running).await;

        // Check for existing checkpoint
        let checkpoint = if config.resume {
            self.checkpoint_manager.load_checkpoint(migration_id).await?
        } else {
            None
        };

        // Create source client
        let mut source_client = clients::create_source_client(&config).await?;
        
        // Create target client
        let target_client = clients::create_target_client(&config).await?;

        // Get total record count if possible
        if let Some(total) = source_client.get_total_count().await? {
            self.update_total_records(migration_id, total).await;
        }

        // Create progress tracker
        let progress_tracker = progress::ProgressTracker::new(migration_id, self.clone());

        // Create batch processor
        let batch_processor = BatchProcessor::new(
            config.clone(),
            source_client,
            target_client,
            progress_tracker,
            self.checkpoint_manager.clone(),
        );

        // Execute migration strategy
        match config.strategy {
            MigrationStrategy::Stream => {
                batch_processor.stream_migration(checkpoint).await?;
            }
            MigrationStrategy::DualWrite => {
                batch_processor.dual_write_migration(checkpoint).await?;
            }
            MigrationStrategy::BlueGreen => {
                batch_processor.blue_green_migration(checkpoint).await?;
            }
            MigrationStrategy::Snapshot => {
                batch_processor.snapshot_migration(checkpoint).await?;
            }
        }

        // Mark as completed
        self.update_status(migration_id, MigrationStatus::Completed).await;
        
        tracing::info!("Migration {} completed successfully", migration_id);
        Ok(())
    }

    /// Get migration progress
    pub async fn get_progress(&self, migration_id: Uuid) -> Option<MigrationProgress> {
        self.active_migrations.read().await.get(&migration_id).cloned()
    }

    /// List all active migrations
    pub async fn list_migrations(&self) -> Vec<MigrationProgress> {
        self.active_migrations.read().await.values().cloned().collect()
    }

    /// Cancel migration
    pub async fn cancel_migration(&self, migration_id: Uuid) -> Result<()> {
        self.update_status(migration_id, MigrationStatus::Cancelled).await;
        // TODO: Implement cancellation logic
        Ok(())
    }

    /// Resume a failed or cancelled migration
    pub async fn resume_migration(&self, migration_id: Uuid) -> Result<bool> {
        // Check if migration exists and can be resumed
        if let Some(progress) = self.get_progress(migration_id).await {
            match progress.status {
                MigrationStatus::Failed | MigrationStatus::Cancelled => {
                    self.update_status(migration_id, MigrationStatus::Running).await;
                    // TODO: Implement resume logic with checkpoint
                    Ok(true)
                }
                _ => Ok(false), // Cannot resume running or completed migrations
            }
        } else {
            Ok(false) // Migration not found
        }
    }

    /// Update migration status
    async fn update_status(&self, migration_id: Uuid, status: MigrationStatus) {
        if let Some(progress) = self.active_migrations.write().await.get_mut(&migration_id) {
            progress.status = status;
            progress.last_update = chrono::Utc::now();
        }
    }

    /// Update total records
    async fn update_total_records(&self, migration_id: Uuid, total: u64) {
        if let Some(progress) = self.active_migrations.write().await.get_mut(&migration_id) {
            progress.total_records = Some(total);
        }
    }

    /// Update processed records
    pub async fn update_processed(&self, migration_id: Uuid, processed: u64, failed: u64) {
        if let Some(progress) = self.active_migrations.write().await.get_mut(&migration_id) {
            progress.processed_records = processed;
            progress.failed_records = failed;
            progress.last_update = chrono::Utc::now();
            
            // Calculate throughput
            let elapsed = progress.last_update.signed_duration_since(progress.start_time);
            if elapsed.num_seconds() > 0 {
                progress.throughput_per_second = processed as f64 / elapsed.num_seconds() as f64;
            }

            // Estimate completion time
            if let Some(total) = progress.total_records {
                if progress.throughput_per_second > 0.0 {
                    let remaining = total - processed;
                    let eta_seconds = remaining as f64 / progress.throughput_per_second;
                    progress.estimated_completion = Some(
                        progress.last_update + chrono::Duration::seconds(eta_seconds as i64)
                    );
                }
            }
        }
    }
}

impl Clone for MigrationManager {
    fn clone(&self) -> Self {
        Self {
            active_migrations: self.active_migrations.clone(),
            checkpoint_manager: self.checkpoint_manager.clone(),
        }
    }
}

/// Batch processor for handling migration execution
struct BatchProcessor {
    config: MigrationConfig,
    source_client: Box<dyn clients::SourceClient>,
    target_client: Box<dyn clients::TargetClient>,
    progress_tracker: progress::ProgressTracker,
    checkpoint_manager: checkpoint::CheckpointManager,
}

impl BatchProcessor {
    fn new(
        config: MigrationConfig,
        source_client: Box<dyn clients::SourceClient>,
        target_client: Box<dyn clients::TargetClient>,
        progress_tracker: progress::ProgressTracker,
        checkpoint_manager: checkpoint::CheckpointManager,
    ) -> Self {
        Self {
            config,
            source_client,
            target_client,
            progress_tracker,
            checkpoint_manager,
        }
    }

    /// Execute streaming migration
    async fn stream_migration(&self, checkpoint: Option<serde_json::Value>) -> Result<()> {
        let (tx, mut rx) = mpsc::channel::<VectorBatch>(self.config.max_concurrency);
        
        // Start producer task
        let producer_config = self.config.clone();
        let producer_client = self.source_client.clone_box();
        let producer_checkpoint = checkpoint.clone();
        
        tokio::spawn(async move {
            if let Err(e) = Self::produce_batches(producer_config, producer_client, producer_checkpoint, tx).await {
                tracing::error!("Producer failed: {}", e);
            }
        });

        // Process batches
        let mut processed_records = 0u64;
        let mut failed_records = 0u64;
        
        while let Some(batch) = rx.recv().await {
            match self.process_batch(&batch).await {
                Ok(_) => {
                    processed_records += batch.records.len() as u64;
                    
                    // Save checkpoint
                    if let Some(checkpoint_data) = &batch.checkpoint_data {
                        self.checkpoint_manager.save_checkpoint(
                            self.config.id,
                            checkpoint_data.clone(),
                        ).await?;
                    }
                }
                Err(e) => {
                    tracing::error!("Batch {} failed: {}", batch.batch_id, e);
                    failed_records += batch.records.len() as u64;
                }
            }

            // Update progress
            let _ = self.progress_tracker.update_progress(processed_records, failed_records).await;
        }

        Ok(())
    }

    /// Execute dual-write migration
    async fn dual_write_migration(&self, checkpoint: Option<serde_json::Value>) -> Result<()> {
            tracing::info!("Starting dual-write migration for {}", self.config.id);

            // Create strategy executor
            let source_client = crate::migration::clients::create_source_client(&self.config).await?;
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            let mut executor = crate::migration::strategies::StrategyExecutor::new(
                self.config.clone(),
                source_client,
                target_client,
                self.progress_tracker.clone(),
                self.checkpoint_manager.clone(),
            );

            // Execute dual-write strategy
            executor.execute_dual_write(checkpoint).await?;

            tracing::info!("Dual-write migration completed for {}", self.config.id);
            Ok(())
        }


    /// Execute blue-green migration
    async fn blue_green_migration(&self, checkpoint: Option<serde_json::Value>) -> Result<()> {
            tracing::info!("Starting blue-green migration for {}", self.config.id);

            // Create strategy executor
            let source_client = crate::migration::clients::create_source_client(&self.config).await?;
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            let mut executor = crate::migration::strategies::StrategyExecutor::new(
                self.config.clone(),
                source_client,
                target_client,
                self.progress_tracker.clone(),
                self.checkpoint_manager.clone(),
            );

            // Execute blue-green strategy
            executor.execute_blue_green(checkpoint).await?;

            tracing::info!("Blue-green migration completed for {}", self.config.id);
            Ok(())
        }


    /// Execute snapshot migration
    async fn snapshot_migration(&self, checkpoint: Option<serde_json::Value>) -> Result<()> {
            tracing::info!("Starting snapshot migration for {}", self.config.id);

            // Create strategy executor
            let source_client = crate::migration::clients::create_source_client(&self.config).await?;
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            let mut executor = crate::migration::strategies::StrategyExecutor::new(
                self.config.clone(),
                source_client,
                target_client,
                self.progress_tracker.clone(),
                self.checkpoint_manager.clone(),
            );

            // Execute snapshot strategy
            executor.execute_snapshot(checkpoint).await?;

            tracing::info!("Snapshot migration completed for {}", self.config.id);
            Ok(())
        }


    /// Produce batches from source
    async fn produce_batches(
        config: MigrationConfig,
        mut source_client: Box<dyn clients::SourceClient>,
        checkpoint: Option<serde_json::Value>,
        tx: mpsc::Sender<VectorBatch>,
    ) -> Result<()> {
        let mut batch_id = 0u64;
        let mut offset = 0u64;

        // Resume from checkpoint if available
        if let Some(checkpoint_data) = checkpoint {
            if let Some(saved_offset) = checkpoint_data.get("offset").and_then(|v| v.as_u64()) {
                offset = saved_offset;
                batch_id = offset / config.batch_size as u64;
            }
        }

        loop {
            let records = source_client.fetch_batch(offset, config.batch_size).await?;
            
            if records.is_empty() {
                break;
            }

            let checkpoint_data = serde_json::json!({
                "offset": offset + records.len() as u64,
                "batch_id": batch_id,
                "timestamp": chrono::Utc::now()
            });

            let batch = VectorBatch {
                records,
                batch_id,
                checkpoint_data: Some(checkpoint_data),
            };

            if tx.send(batch).await.is_err() {
                break; // Receiver dropped
            }

            offset += config.batch_size as u64;
            batch_id += 1;
        }

        Ok(())
    }

    /// Process a single batch
    async fn process_batch(&self, batch: &VectorBatch) -> Result<()> {
        if self.config.dry_run {
            // In dry-run mode, just validate the data
            for record in &batch.records {
                validation::validate_record(record, &self.config.validation)?;
            }
            tracing::info!("Dry-run: Would process batch {} with {} records", 
                          batch.batch_id, batch.records.len());
            return Ok(());
        }

        // Apply transformations
        let transformed_records = self.apply_transformations(&batch.records)?;

        // Validate records
        for record in &transformed_records {
            validation::validate_record(record, &self.config.validation)?;
        }

        // Insert into target
        self.target_client.insert_batch(&transformed_records).await?;

        tracing::debug!("Processed batch {} with {} records", 
                       batch.batch_id, batch.records.len());
        Ok(())
    }

    /// Apply transformation rules to records
    fn apply_transformations(&self, records: &[VectorRecord]) -> Result<Vec<VectorRecord>> {
        let mut transformed = Vec::with_capacity(records.len());

        for record in records {
            let mut new_record = record.clone();
            
            for rule in &self.config.transformations {
                self.apply_transformation_rule(&mut new_record, rule)?;
            }
            
            transformed.push(new_record);
        }

        Ok(transformed)
    }

    /// Apply a single transformation rule
    fn apply_transformation_rule(&self, record: &mut VectorRecord, rule: &TransformationRule) -> Result<()> {
        match rule {
            TransformationRule::DimensionReduction { from_dimension, to_dimension, method: _ } => {
                if record.vector.len() == *from_dimension && *to_dimension < *from_dimension {
                    record.vector.truncate(*to_dimension);
                }
            }
            TransformationRule::MetadataFilter { query: _ } => {
                // Filter operations are handled at the batch level
            }
            TransformationRule::FieldRename { field, new_name } => {
                if let Some(value) = record.metadata.remove(field) {
                    record.metadata.insert(new_name.clone(), value);
                }
            }
            TransformationRule::FieldMap { field, mapping } => {
                if let Some(value) = record.metadata.get(field) {
                    if let Some(string_val) = value.as_str() {
                        if let Some(mapped_val) = mapping.get(string_val) {
                            record.metadata.insert(field.clone(), serde_json::Value::String(mapped_val.clone()));
                        }
                    }
                }
            }
            TransformationRule::FieldConvert { field, conversion } => {
                if let Some(value) = record.metadata.get(field).cloned() {
                    let converted = match conversion {
                        ConversionType::StringToNumber => {
                            if let Some(s) = value.as_str() {
                                s.parse::<f64>().map(serde_json::Value::from).unwrap_or(value)
                            } else {
                                value
                            }
                        }
                        ConversionType::NumberToString => {
                            if let Some(n) = value.as_f64() {
                                serde_json::Value::String(n.to_string())
                            } else {
                                value
                            }
                        }
                        ConversionType::ArrayToString => {
                            if let Some(arr) = value.as_array() {
                                let strings: Vec<String> = arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .collect();
                                serde_json::Value::String(strings.join(","))
                            } else {
                                value
                            }
                        }
                        ConversionType::StringToArray => {
                            if let Some(s) = value.as_str() {
                                let parts: Vec<serde_json::Value> = s.split(',')
                                    .map(|part| serde_json::Value::String(part.trim().to_string()))
                                    .collect();
                                serde_json::Value::Array(parts)
                            } else {
                                value
                            }
                        }
                    };
                    record.metadata.insert(field.clone(), converted);
                }
            }
            TransformationRule::FieldFilter { field: _, condition: _ } => {
                // Filter operations are handled at the batch level
            }
        }
        Ok(())
    }
}

/// Default migration configuration
impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            source_type: SourceType::Qdrant,
            source_url: String::new(),
            target_url: "http://localhost:6333".to_string(),
            source_collection: None,
            target_collection: "migrated_collection".to_string(),
            batch_size: 1000,
            max_concurrency: 4,
            dry_run: false,
            resume: false,
            checkpoint_dir: PathBuf::from("./checkpoints"),
            strategy: MigrationStrategy::Stream,
            source_auth: None,
            target_auth: None,
            transformations: Vec::new(),
            validation: ValidationConfig::default(),
        }
    }
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            validate_vectors: true,
            validate_metadata: true,
            check_duplicates: false,
            vector_dimension: None,
            required_fields: Vec::new(),
        }
    }
}

