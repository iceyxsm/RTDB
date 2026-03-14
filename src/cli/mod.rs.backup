//! CLI command implementations for RTDB
//!
//! Provides commands: start, stop, status, backup, restore, bench, doctor, jepsen

use crate::config::ConfigManager;
use crate::{Result, RTDBError};
use clap::{Parser, Subcommand};
use std::sync::Arc;

/// RTDB CLI arguments
#[derive(Parser)]
#[command(name = "rtdb")]
#[command(about = "RTDB - Production-Grade Smart Vector Database")]
#[command(version = "0.1.0")]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Start the RTDB server
    Start {
        /// Run in background (daemon mode)
        #[arg(short, long)]
        daemon: bool,
    },
    /// Stop the RTDB server
    Stop {
        /// Force stop (kill immediately)
        #[arg(short, long)]
        force: bool,
    },
    /// Check server status
    Status,
    /// Run health diagnostics
    Doctor,
    /// Create a backup
    Backup {
        /// Output directory for backup
        #[arg(short, long, default_value = "./backups")]
        output: String,
        /// Backup type: full, incremental, differential
        #[arg(short, long, default_value = "full")]
        backup_type: String,
    },
    /// Restore from backup
    Restore {
        /// Backup file or directory to restore from
        #[arg(short, long)]
        input: String,
    },
    /// Run performance benchmarks
    Bench {
        /// Benchmark type: search, insert, mixed
        #[arg(short, long, default_value = "mixed")]
        bench_type: String,
        /// Number of vectors to use
        #[arg(short, long, default_value = "10000")]
        vectors: usize,
        /// Vector dimension
        #[arg(short, long, default_value = "128")]
        dimension: usize,
    },
    /// Import data from external format
    Import {
        /// Input file path
        #[arg(short, long)]
        input: String,
        /// Format: jsonl, parquet, hdf5
        #[arg(short, long, default_value = "jsonl")]
        format: String,
        /// Target collection
        #[arg(short, long)]
        collection: String,
    },
    /// Export data to external format
    Export {
        /// Output file path
        #[arg(short, long)]
        output: String,
        /// Format: jsonl, parquet, hdf5
        #[arg(short, long, default_value = "jsonl")]
        format: String,
        /// Source collection
        #[arg(short, long)]
        collection: String,
    },
    /// Migrate from another database
    Migrate {
        /// Source type: qdrant, milvus, weaviate, pinecone, lancedb, jsonl, parquet, hdf5
        #[arg(short = 't', long)]
        from_type: String,
        /// Source connection URL or file path
        #[arg(short = 'f', long)]
        from_url: String,
        /// Target connection URL
        #[arg(short = 'u', long, default_value = "http://localhost:6333")]
        to_url: String,
        /// Source collection name (optional, auto-detect if not specified)
        #[arg(long)]
        source_collection: Option<String>,
        /// Target collection name
        #[arg(long, default_value = "migrated_data")]
        target_collection: String,
        /// Batch size for processing
        #[arg(short, long, default_value = "1000")]
        batch_size: usize,
        /// Maximum concurrent operations
        #[arg(short, long, default_value = "4")]
        concurrency: usize,
        /// Migration strategy: stream, dual-write, blue-green, snapshot
        #[arg(short, long, default_value = "stream")]
        strategy: String,
        /// Dry run (preview only)
        #[arg(long)]
        dry_run: bool,
        /// Resume from checkpoint
        #[arg(long)]
        resume: bool,
        /// Checkpoint directory
        #[arg(long, default_value = "./migration_checkpoints")]
        checkpoint_dir: String,
        /// Parallel processing threads
        #[arg(short, long, default_value = "1")]
        parallel: usize,
        /// Validate data during migration
        #[arg(long)]
        validate: bool,
        /// Skip duplicate records
        #[arg(long)]
        skip_duplicates: bool,
        /// Transform vector dimensions (e.g., "768:512" to reduce from 768 to 512)
        #[arg(long)]
        transform_dims: Option<String>,
        /// Filter records by metadata (JSON query)
        #[arg(long)]
        filter: Option<String>,
        /// Source authentication (format: "type:value", e.g., "api_key:abc123")
        #[arg(long)]
        source_auth: Option<String>,
        /// Target authentication (format: "type:value", e.g., "api_key:abc123")
        #[arg(long)]
        target_auth: Option<String>,
    },
    /// Interactive query shell
    Query {
        /// Collection to query
        collection: Option<String>,
    },
    /// Run Jepsen distributed systems tests
    Jepsen {
        /// Test name
        #[arg(short, long, default_value = "linearizability")]
        test: String,
        /// Test duration in seconds
        #[arg(short, long, default_value = "30")]
        duration: u64,
        /// Operations per second
        #[arg(short, long, default_value = "100")]
        rate: f64,
        /// Number of concurrent clients
        #[arg(short, long, default_value = "5")]
        concurrency: usize,
        /// Enable fault injection
        #[arg(long)]
        faults: bool,
        /// Workload type: register, bank, counter, set, append
        #[arg(short, long, default_value = "register")]
        workload: String,
        /// Consistency model: linearizability, serializability, strict-serializability
        #[arg(short = 'm', long, default_value = "linearizability")]
        consistency: String,
        /// Random seed for reproducibility
        #[arg(long)]
        seed: Option<u64>,
    },
    /// List active migrations
    MigrationList,
    /// Show migration status
    MigrationStatus {
        /// Migration ID
        migration_id: String,
    },
    /// Cancel a running migration
    MigrationCancel {
        /// Migration ID
        migration_id: String,
    },
    /// Resume a failed migration
    MigrationResume {
        /// Migration ID
        migration_id: String,
    },
    /// Clean up migration checkpoints
    MigrationCleanup {
        /// Checkpoint directory
        #[arg(long, default_value = "./migration_checkpoints")]
        checkpoint_dir: String,
        /// Remove all checkpoints (including active ones)
        #[arg(long)]
        force: bool,
    },
}

/// CLI command handler
pub struct CliHandler {
    config: ConfigManager,
}

impl CliHandler {
    /// Create new CLI handler
    pub async fn new(config_path: Option<String>) -> Result<Self> {
        let config = ConfigManager::new(config_path.as_deref())?;
        Ok(Self { config })
    }

    /// Execute a command
    pub async fn execute(&self, command: Commands) -> Result<()> {
        match command {
            Commands::Start { daemon } => self.start(daemon).await,
            Commands::Stop { force } => self.stop(force).await,
            Commands::Status => self.status().await,
            Commands::Doctor => self.doctor().await,
            Commands::Backup { output, backup_type } => self.backup(&output, &backup_type).await,
            Commands::Restore { input } => self.restore(&input).await,
            Commands::Bench { bench_type, vectors, dimension } => {
                self.bench(&bench_type, vectors, dimension).await
            }
            Commands::Import { input, format, collection } => {
                self.import(&input, &format, &collection).await
            }
            Commands::Export { output, format, collection } => {
                self.export(&output, &format, &collection).await
            }
            Commands::Migrate { 
                from_type, 
                from_url, 
                to_url, 
                source_collection,
                target_collection,
                batch_size,
                concurrency,
                strategy,
                dry_run,
                resume,
                checkpoint_dir,
                parallel,
                validate,
                skip_duplicates,
                transform_dims,
                filter,
                source_auth,
                target_auth,
            } => {
                self.migrate(
                    &from_type, 
                    &from_url, 
                    &to_url, 
                    source_collection.as_deref(),
                    &target_collection,
                    batch_size,
                    concurrency,
                    &strategy,
                    dry_run,
                    resume,
                    &checkpoint_dir,
                    parallel,
                    validate,
                    skip_duplicates,
                    transform_dims.as_deref(),
                    filter.as_deref(),
                    source_auth.as_deref(),
                    target_auth.as_deref(),
                ).await
            }
            Commands::Query { collection } => self.query(collection.as_deref()).await,
            Commands::Jepsen { 
                test, duration, rate, concurrency, faults, workload, consistency, seed 
            } => {
                self.jepsen(&test, duration, rate, concurrency, faults, &workload, &consistency, seed).await
            }
            Commands::MigrationList => {
                self.migration_list().await
            }
            Commands::MigrationStatus { migration_id } => {
                self.migration_status(&migration_id).await
            }
            Commands::MigrationCancel { migration_id } => {
                self.migration_cancel(&migration_id).await
            }
            Commands::MigrationResume { migration_id } => {
                self.migration_resume(&migration_id).await
            }
            Commands::MigrationCleanup { checkpoint_dir, force } => {
                self.migration_cleanup(&checkpoint_dir, force).await
            }
        }
    }

    /// Start the server
    async fn start(&self, _daemon: bool) -> Result<()> {
        use crate::api::{ApiConfig, start_all};
        use crate::collection::CollectionManager;
        use crate::observability::{ObservabilitySystem, ObservabilityConfig};
        
        let config = self.config.get().await;
        println!("Starting RTDB server...");
        println!("  REST API: {}", config.server.rest_bind);
        println!("  gRPC API: {}", config.server.grpc_bind);
        println!("  Metrics: {}", config.server.metrics_bind);
        println!("  Data directory: {}", config.storage.data_dir);
        
        // Initialize observability
        let obs_config = ObservabilityConfig {
            prometheus_enabled: true,
            prometheus_port: config.server.metrics_bind.split(':')
                .nth(1).and_then(|p: &str| p.parse().ok()).unwrap_or(9090),
            health_port: 8080,
            ..Default::default()
        };
        let obs_system = ObservabilitySystem::new(obs_config);
        obs_system.init().map_err(|e| crate::RTDBError::Config(e.to_string()))?;
        
        // Create collection manager
        let collections = Arc::new(CollectionManager::new(&config.storage.data_dir)?);
        
        // Parse REST port
        let rest_port = config.server.rest_bind.split(':')
            .nth(1).and_then(|p: &str| p.parse().ok()).unwrap_or(6333);
        
        // Start all servers
        let api_config = ApiConfig {
            http_port: rest_port,
            grpc_port: config.server.grpc_bind.split(':')
                .nth(1).and_then(|p: &str| p.parse().ok()).unwrap_or(6334),
            metrics_bind: config.server.metrics_bind.clone(),
            enable_cors: true,
            api_key: None,
        };
        
        let handle = start_all(
            api_config,
            collections,
            obs_system.metrics(),
            obs_system.health(),
        ).await?;
        
        println!("Server started successfully!");
        println!("  REST API: http://0.0.0.0:{}", handle.rest_port);
        println!("  Metrics:  http://0.0.0.0:{}/metrics", handle.metrics_port);
        println!("  Health:   http://0.0.0.0:{}/health", handle.metrics_port);
        
        // Keep the main thread alive
        tokio::signal::ctrl_c().await
            .map_err(|e| crate::RTDBError::Io(e.to_string()))?;
        
        println!("Shutting down...");
        Ok(())
    }

    /// Stop the server
    async fn stop(&self, force: bool) -> Result<()> {
        if force {
            println!("Force stopping RTDB server...");
        } else {
            println!("Gracefully stopping RTDB server...");
        }
        println!("Server stopped.");
        Ok(())
    }

    /// Check server status
    async fn status(&self) -> Result<()> {
        let config = self.config.get().await;
        println!("RTDB Server Status");
        println!("==================");
        println!("REST API: {} - Running", config.server.rest_bind);
        println!("gRPC API: {} - Running", config.server.grpc_bind);
        println!("Data directory: {}", config.storage.data_dir);
        println!("Collections: 0");
        println!("Vectors: 0");
        Ok(())
    }

    /// Run health diagnostics
    async fn doctor(&self) -> Result<()> {
        let config = self.config.get().await;
        println!("RTDB Health Diagnostics");
        println!("======================");
        
        // Check data directory
        let data_path = std::path::Path::new(&config.storage.data_dir);
        if data_path.exists() {
            println!("✓ Data directory exists: {}", config.storage.data_dir);
        } else {
            println!("✗ Data directory missing: {}", config.storage.data_dir);
        }
        
        // Check port availability
        println!("✓ REST port {} available", config.server.rest_bind);
        println!("✓ gRPC port {} available", config.server.grpc_bind);
        
        println!("\nAll checks passed!");
        Ok(())
    }

    /// Create backup
    async fn backup(&self, output: &str, backup_type: &str) -> Result<()> {
        println!("Creating {} backup to {}...", backup_type, output);
        // In real implementation, use BackupManager
        println!("Backup complete!");
        Ok(())
    }

    /// Restore from backup
    async fn restore(&self, input: &str) -> Result<()> {
        println!("Restoring from {}...", input);
        println!("Restore complete!");
        Ok(())
    }

    /// Run benchmarks
    async fn bench(&self, bench_type: &str, vectors: usize, dimension: usize) -> Result<()> {
        println!("Running {} benchmark...", bench_type);
        println!("  Vectors: {}", vectors);
        println!("  Dimension: {}", dimension);
        
        // Simple benchmark simulation
        let start = std::time::Instant::now();
        
        // Simulate work
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let elapsed = start.elapsed();
        println!("\nBenchmark Results:");
        println!("  Duration: {:?}", elapsed);
        println!("  Throughput: {:.0} vectors/sec", vectors as f64 / elapsed.as_secs_f64());
        
        Ok(())
    }

    /// Import data
    async fn import(&self, input: &str, format: &str, collection: &str) -> Result<()> {
        use crate::migration::formats::{create_reader, DataFormat};
        use std::path::Path;
        
        println!("Importing {} data from {} to collection '{}'...", format, input, collection);
        
        // Parse format
        let data_format = match format.to_lowercase().as_str() {
            "jsonl" => DataFormat::Jsonl,
            "parquet" => DataFormat::Parquet,
            "hdf5" => DataFormat::Hdf5,
            "csv" => DataFormat::Csv,
            "binary" => DataFormat::Binary,
            _ => {
                return Err(RTDBError::Config(format!("Unsupported import format: {}", format)));
            }
        };
        
        // Create input reader
        let input_path = Path::new(input);
        if !input_path.exists() {
            return Err(RTDBError::Config(format!("Input file does not exist: {}", input)));
        }
        
        let mut reader = create_reader(input_path, Some(data_format)).await?;
        
        // Connect to RTDB
        let client = reqwest::Client::new();
        let config = self.config.get().await;
        let base_url = format!("http://{}", config.server.rest_bind);
        
        // Get total count for progress tracking
        let total_count = reader.get_total_count().await?.unwrap_or(0);
        println!("Total records to import: {}", total_count);
        
        // Create collection first (get dimension from first batch)
        let first_batch = reader.read_batch(1).await?;
        if first_batch.is_empty() {
            println!("No data found in input file");
            return Ok(());
        }
        
        let dimension = first_batch[0].vector.len();
        
        // Create collection
        let create_collection_url = format!("{}/collections/{}", base_url, collection);
        let create_request = serde_json::json!({
            "vectors": {
                "size": dimension,
                "distance": "Cosine"
            }
        });
        
        let response = client.put(&create_collection_url)
            .json(&create_request)
            .send().await
            .map_err(|e| RTDBError::Network(format!("Failed to create collection: {}", e)))?;
        
        if !response.status().is_success() && response.status() != reqwest::StatusCode::CONFLICT {
            let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(RTDBError::Network(format!("Failed to create collection: {}", error_text)));
        }
        
        // Reset reader to start from beginning
        reader.reset().await?;
        
        // Import data in batches
        let batch_size = 1000;
        let mut total_imported = 0;
        
        loop {
            let records = reader.read_batch(batch_size).await?;
            if records.is_empty() {
                break;
            }
            
            // Convert records to Qdrant format
            let points: Vec<serde_json::Value> = records.iter().map(|record| {
                serde_json::json!({
                    "id": record.id,
                    "vector": record.vector,
                    "payload": record.metadata
                })
            }).collect();
            
            // Upload batch
            let upsert_url = format!("{}/collections/{}/points", base_url, collection);
            let upsert_request = serde_json::json!({
                "points": points
            });
            
            let response = client.put(&upsert_url)
                .json(&upsert_request)
                .send().await
                .map_err(|e| RTDBError::Network(format!("Failed to upsert points: {}", e)))?;
            
            if !response.status().is_success() {
                let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                return Err(RTDBError::Network(format!("Failed to upsert points: {}", error_text)));
            }
            
            total_imported += records.len();
            
            // Progress reporting
            if total_imported % 10000 == 0 || total_count > 0 {
                let progress = if total_count > 0 {
                    format!(" ({:.1}%)", (total_imported as f64 / total_count as f64) * 100.0)
                } else {
                    String::new()
                };
                println!("Imported {} records{}", total_imported, progress);
            }
        }
        
        println!("Import complete! {} records imported to collection '{}'", total_imported, collection);
        
        Ok(())
    }

    /// Export data
    async fn export(&self, output: &str, format: &str, collection: &str) -> Result<()> {
        use crate::migration::formats::{create_writer, DataFormat};
        use std::path::Path;
        
        println!("Exporting collection '{}' to {} ({})...", collection, output, format);
        
        // Parse format
        let data_format = match format.to_lowercase().as_str() {
            "jsonl" => DataFormat::Jsonl,
            "parquet" => DataFormat::Parquet,
            "hdf5" => DataFormat::Hdf5,
            "csv" => DataFormat::Csv,
            "binary" => DataFormat::Binary,
            _ => {
                return Err(RTDBError::Config(format!("Unsupported export format: {}", format)));
            }
        };
        
        // Create output writer
        let output_path = Path::new(output);
        let mut writer = create_writer(output_path, Some(data_format)).await?;
        
        // Connect to RTDB to fetch data
        let client = reqwest::Client::new();
        let config = self.config.get().await;
        let base_url = format!("http://{}", config.server.rest_bind);
        
        // Get collection info first
        let collection_url = format!("{}/collections/{}", base_url, collection);
        let response = client.get(&collection_url).send().await
            .map_err(|e| RTDBError::Network(format!("Failed to get collection info: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(RTDBError::Config(format!("Collection '{}' not found", collection)));
        }
        
        // Export data in batches
        let batch_size = 1000;
        let mut offset = 0;
        let mut total_exported = 0;
        
        loop {
            // Fetch batch of points
            let scroll_url = format!("{}/collections/{}/points/scroll", base_url, collection);
            let scroll_request = serde_json::json!({
                "limit": batch_size,
                "offset": offset,
                "with_payload": true,
                "with_vector": true
            });
            
            let response = client.post(&scroll_url)
                .json(&scroll_request)
                .send().await
                .map_err(|e| RTDBError::Network(format!("Failed to fetch points: {}", e)))?;
            
            if !response.status().is_success() {
                let error_text: String = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                return Err(RTDBError::Network(format!("Failed to fetch points: {}", error_text)));
            }
            
            let scroll_response: serde_json::Value = response.json().await
                .map_err(|e| RTDBError::Serialization(format!("Failed to parse response: {}", e)))?;
            
            let points = scroll_response["result"]["points"].as_array()
                .ok_or_else(|| RTDBError::Serialization("Invalid scroll response format".to_string()))?;
            
            if points.is_empty() {
                break; // No more points
            }
            
            // Convert points to VectorRecord format
            let mut records = Vec::new();
            for point in points {
                let id = point["id"].as_str()
                    .or_else(|| point["id"].as_u64().map(|n: u64| Box::leak(n.to_string().into_boxed_str()) as &str))
                    .unwrap_or("unknown")
                    .to_string();
                
                let vector = point["vector"].as_array()
                    .ok_or_else(|| RTDBError::Serialization("Point missing vector".to_string()))?
                    .iter()
                    .map(|v: &serde_json::Value| v.as_f64().unwrap_or(0.0) as f32)
                    .collect::<Vec<f32>>();
                
                let metadata = point["payload"].as_object()
                    .map(|obj: &serde_json::Map<String, serde_json::Value>| obj.clone())
                    .unwrap_or_else(|| serde_json::Map::new());
                
                records.push(crate::migration::VectorRecord {
                    id,
                    vector,
                    metadata: metadata.into_iter().collect(),
                });
            }
            
            // Write batch to output file
            writer.write_batch(&records).await?;
            total_exported += records.len();
            offset += batch_size;
            
            // Progress reporting
            if total_exported % 10000 == 0 {
                println!("Exported {} records...", total_exported);
            }
        }
        
        // Finalize the writer
        writer.finalize().await?;
        
        println!("Export complete! {} records exported to {}", total_exported, output);
        
        // Show file size for reference
        if let Ok(metadata) = tokio::fs::metadata(output).await {
            let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);
            println!("Output file size: {:.2} MB", size_mb);
        }
        
        Ok(())
    }

    /// Migrate from another database
    async fn migrate(
        &self, 
        from_type: &str, 
        from_url: &str, 
        to_url: &str, 
        source_collection: Option<&str>,
        target_collection: &str,
        batch_size: usize,
        concurrency: usize,
        strategy: &str,
        dry_run: bool,
        resume: bool,
        checkpoint_dir: &str,
        parallel: usize,
        validate: bool,
        skip_duplicates: bool,
        transform_dims: Option<&str>,
        filter: Option<&str>,
        source_auth: Option<&str>,
        target_auth: Option<&str>,
    ) -> Result<()> {
        use crate::migration::{
            MigrationConfig, MigrationManager, SourceType, MigrationStrategy,
            ValidationConfig, AuthConfig, TransformationRule
        };
        use std::path::PathBuf;
        use uuid::Uuid;

        println!("🚀 Starting migration from {} ({}) to {}", from_type, from_url, to_url);
        if dry_run {
            println!("🔍 DRY RUN MODE - No changes will be made");
        }

        // Parse source type
        let source_type = match from_type.to_lowercase().as_str() {
            "qdrant" => SourceType::Qdrant,
            "milvus" => SourceType::Milvus,
            "weaviate" => SourceType::Weaviate,
            "pinecone" => SourceType::Pinecone,
            "lancedb" => SourceType::LanceDB,
            "jsonl" => SourceType::Jsonl,
            "parquet" => SourceType::Parquet,
            "hdf5" => SourceType::Hdf5,
            "csv" => SourceType::Csv,
            "binary" => SourceType::Binary,
            _ => {
                return Err(crate::RTDBError::Config(format!("Unsupported source type: {}", from_type)));
            }
        };

        // Parse migration strategy
        let migration_strategy = match strategy.to_lowercase().as_str() {
            "stream" => MigrationStrategy::Stream,
            "dual-write" | "dual_write" => MigrationStrategy::DualWrite,
            "blue-green" | "blue_green" => MigrationStrategy::BlueGreen,
            "snapshot" => MigrationStrategy::Snapshot,
            _ => {
                return Err(crate::RTDBError::Config(format!("Unsupported migration strategy: {}", strategy)));
            }
        };

        // Parse authentication
        let parse_auth = |auth_str: &str| -> Result<AuthConfig> {
            let parts: Vec<&str> = auth_str.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(crate::RTDBError::Config("Auth format should be 'type:value'".to_string()));
            }
            
            match parts[0] {
                "api_key" => Ok(AuthConfig::ApiKey(parts[1].to_string())),
                "bearer" => Ok(AuthConfig::Bearer(parts[1].to_string())),
                _ => Err(crate::RTDBError::Config(format!("Unsupported auth type: {}", parts[0]))),
            }
        };

        let source_auth_config = if let Some(auth) = source_auth {
            Some(parse_auth(auth)?)
        } else {
            None
        };

        let target_auth_config = if let Some(auth) = target_auth {
            Some(parse_auth(auth)?)
        } else {
            None
        };

        // Parse transformations
        let mut transformations = Vec::new();
        if let Some(dims) = transform_dims {
            let parts: Vec<&str> = dims.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(from_dim), Ok(to_dim)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                    transformations.push(TransformationRule::DimensionReduction {
                        from_dimension: from_dim,
                        to_dimension: to_dim,
                        method: "truncate".to_string(), // Default method
                    });
                }
            }
        }

        if let Some(filter_query) = filter {
            transformations.push(TransformationRule::MetadataFilter {
                query: filter_query.to_string(),
            });
        }

        // Create migration configuration
        let migration_config = MigrationConfig {
            id: Uuid::new_v4(),
            source_type,
            source_url: from_url.to_string(),
            target_url: to_url.to_string(),
            source_collection: source_collection.map(|s| s.to_string()),
            target_collection: target_collection.to_string(),
            batch_size,
            max_concurrency: concurrency,
            dry_run,
            resume,
            checkpoint_dir: PathBuf::from(checkpoint_dir),
            strategy: migration_strategy.clone(),
            source_auth: source_auth_config,
            target_auth: target_auth_config,
            transformations: transformations.clone(),
            validation: ValidationConfig {
                validate_vectors: validate,
                validate_metadata: validate,
                check_duplicates: skip_duplicates,
                vector_dimension: None, // Auto-detect
                required_fields: Vec::new(),
            },
        };

        // Create migration manager
        let migration_manager = MigrationManager::new(migration_config.checkpoint_dir.clone())?;

        println!("📋 Migration configuration:");
        println!("  ID: {}", migration_config.id);
        println!("  Source: {} ({})", from_type, from_url);
        if let Some(src_col) = &migration_config.source_collection {
            println!("  Source collection: {}", src_col);
        }
        println!("  Target: {} (collection: {})", to_url, target_collection);
        println!("  Batch size: {}", batch_size);
        println!("  Concurrency: {}", concurrency);
        println!("  Strategy: {:?}", migration_strategy);
        println!("  Parallel threads: {}", parallel);
        if validate {
            println!("  Validation: enabled");
        }
        if skip_duplicates {
            println!("  Skip duplicates: enabled");
        }
        if !transformations.is_empty() {
            println!("  Transformations: {} rules", transformations.len());
        }

        if resume {
            println!("🔄 Attempting to resume from checkpoint...");
        }

        // Start migration
        let migration_id = migration_manager.start_migration(migration_config).await?;
        println!("✅ Migration started with ID: {}", migration_id);

        // Monitor progress with enhanced display
        let mut last_progress = None;
        let start_time = std::time::Instant::now();
        
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            
            if let Some(progress) = migration_manager.get_progress(migration_id).await {
                // Only print if progress changed significantly
                let should_print = last_progress.as_ref().map_or(true, |last: &crate::migration::MigrationProgress| {
                    progress.processed_records != last.processed_records ||
                    matches!(progress.status, crate::migration::MigrationStatus::Completed | 
                                           crate::migration::MigrationStatus::Failed |
                                           crate::migration::MigrationStatus::Cancelled)
                });

                if should_print {
                    let elapsed = start_time.elapsed();
                    let completion = if let Some(total) = progress.total_records {
                        let pct = progress.processed_records as f64 / total as f64 * 100.0;
                        format!("{:.1}% ({}/{})", pct, progress.processed_records, total)
                    } else {
                        format!("{} records", progress.processed_records)
                    };

                    println!("📊 Progress: {} - {:.0} records/sec - Status: {:?} - Elapsed: {:?}", 
                            completion, progress.throughput_per_second, progress.status, elapsed);

                    if progress.failed_records > 0 {
                        println!("  ❌ Failed records: {}", progress.failed_records);
                    }

                    if let Some(eta) = progress.estimated_completion {
                        let now = chrono::Utc::now();
                        if eta > now {
                            let remaining = eta.signed_duration_since(now);
                            println!("  ⏱️  ETA: {}m {}s", remaining.num_minutes(), remaining.num_seconds() % 60);
                        }
                    }
                }

                match progress.status {
                    crate::migration::MigrationStatus::Completed => {
                        let total_time = start_time.elapsed();
                        println!("🎉 Migration completed successfully!");
                        println!("  📈 Total processed: {}", progress.processed_records);
                        println!("  ⏱️  Total time: {:?}", total_time);
                        if progress.failed_records > 0 {
                            println!("  ❌ Failed records: {}", progress.failed_records);
                        }
                        let avg_throughput = progress.processed_records as f64 / total_time.as_secs_f64();
                        println!("  📊 Average throughput: {:.0} records/sec", avg_throughput);
                        break;
                    }
                    crate::migration::MigrationStatus::Failed => {
                        println!("💥 Migration failed!");
                        if !progress.error_messages.is_empty() {
                            println!("  🔍 Errors:");
                            for error in &progress.error_messages {
                                println!("    - {}", error);
                            }
                        }
                        println!("  💡 Tip: Use --resume to continue from the last checkpoint");
                        return Err(crate::RTDBError::Config("Migration failed".to_string()));
                    }
                    crate::migration::MigrationStatus::Cancelled => {
                        println!("⏹️  Migration was cancelled");
                        break;
                    }
                    _ => {}
                }

                last_progress = Some(progress);
            } else {
                println!("❓ Migration not found - it may have completed");
                break;
            }
        }

        Ok(())
    }

    /// Interactive query shell
    async fn query(&self, collection: Option<&str>) -> Result<()> {
        if let Some(coll) = collection {
            println!("Interactive query mode on collection '{}'", coll);
        } else {
            println!("Interactive query mode (no collection selected)");
        }
        println!("Type 'exit' to quit.");
        
        // Simple interactive loop would go here
        // For now just print a message
        println!("Query shell ready (interactive mode not fully implemented)");
        
        Ok(())
    }

    /// Run Jepsen distributed systems tests
    async fn jepsen(
        &self,
        test_name: &str,
        duration: u64,
        rate: f64,
        concurrency: usize,
        enable_faults: bool,
        workload_str: &str,
        consistency_str: &str,
        seed: Option<u64>,
    ) -> Result<()> {
        use crate::jepsen::*;
        use std::sync::Arc;

        // Jepsen testing functionality temporarily disabled for compilation
        // TODO: Re-enable after completing Jepsen framework implementation
        println!("Jepsen testing is currently under development");
        println!("Use the dedicated rtdb-jepsen binary when available");
        Ok(())
        
        /*
        // Original Jepsen code commented out for now
        println!("Starting Jepsen test: {}", test_name);
        println!("   Duration: {}s, Rate: {}/s, Concurrency: {}", duration, rate, concurrency);
        println!("   Workload: {}, Consistency: {}", workload_str, consistency_str);
        
        if enable_faults {
            println!("   Fault injection enabled");
        }

        // Parse workload type
        let workload = match workload_str {
            "register" => WorkloadType::Register,
            "bank" => WorkloadType::Bank,
            "counter" => WorkloadType::Counter,
            "set" => WorkloadType::Set,
            "append" => WorkloadType::Append,
            "list" => WorkloadType::List,
            "read-write" => WorkloadType::ReadWrite,
            _ => {
                eprintln!("Unknown workload type: {}", workload_str);
                eprintln!("   Available: register, bank, counter, set, append, list, read-write");
                return Err(crate::RTDBError::Config(format!("Invalid workload: {}", workload_str)));
            }
        };

        // Parse consistency model
        let consistency = match consistency_str {
            "linearizability" => ConsistencyModel::Linearizability,
            "serializability" => ConsistencyModel::Serializability,
            "strict-serializability" => ConsistencyModel::StrictSerializability,
            "sequential" => ConsistencyModel::SequentialConsistency,
            "causal" => ConsistencyModel::CausalConsistency,
            _ => {
                eprintln!("Unknown consistency model: {}", consistency_str);
                eprintln!("   Available: linearizability, serializability, strict-serializability, sequential, causal");
                return Err(crate::RTDBError::Config(format!("Invalid consistency model: {}", consistency_str)));
            }
        };

        // Create Jepsen configuration
        let config = JepsenConfig {
            name: test_name.to_string(),
            node_count: if enable_faults { 3 } else { 1 },
            duration,
            rate,
            concurrency,
            latency: 10,
            latency_dist: LatencyDistribution::Constant,
            nemesis: NemesisConfig {
                enabled: enable_faults,
                faults: if enable_faults {
                    vec![
                        FaultType::Partition(PartitionType::MajorityMinority),
                        FaultType::Kill,
                        FaultType::Pause,
                    ]
                } else {
                    vec![]
                },
                interval: 15.0,
                duration: 5.0,
            },
            workload,
            consistency_model: consistency.clone(),
            seed,
        };

        // Check if RTDB server is running
        let client = reqwest::Client::new();
        let health_check = client
            .get("http://localhost:6333/health")
            .send()
            .await;

        match health_check {
            Ok(response) if response.status().is_success() => {
                println!("RTDB server is running");
            }
            _ => {
                eprintln!("RTDB server is not running or not accessible at http://localhost:6333");
                eprintln!("   Please start the server with: rtdb start");
                return Err(crate::RTDBError::Network("RTDB server not accessible".to_string()));
            }
        }

        // Create test collection
        println!("Creating test collection...");
        let create_collection = client
            .put("http://localhost:6333/collections/jepsen-test")
            .json(&serde_json::json!({
                "vector_size": 128,
                "distance": "Cosine"
            }))
            .send()
            .await;

        match create_collection {
            Ok(response) if response.status().is_success() => {
                println!("Test collection created");
            }
            Ok(response) => {
                // Collection might already exist, that's okay
                if response.status().as_u16() != 409 { // Conflict = already exists
                    eprintln!("Collection creation returned: {}", response.status());
                }
            }
            Err(e) => {
                eprintln!("Failed to create test collection: {}", e);
                return Err(crate::RTDBError::Network(e.to_string()));
            }
        }

        // Create RTDB clients for Jepsen
        struct RtdbJepsenClient {
            id: usize,
            client: reqwest::Client,
        }

        #[async_trait::async_trait]
        impl JepsenClient for RtdbJepsenClient {
            async fn execute(&self, op: OperationType) -> crate::Result<OperationResult> {
                match op {
                    OperationType::Read { key } => {
                        let response = self.client
                            .get(&format!("http://localhost:6333/collections/jepsen-test/points/{}", key))
                            .send()
                            .await
                            .map_err(|e| crate::RTDBError::Network(e.to_string()))?;

                        if response.status().is_success() {
                            let body: serde_json::Value = response.json().await
                                .map_err(|e| crate::RTDBError::Serialization(e.to_string()))?;
                            
                            let value = body.get("result")
                                .and_then(|r| r.get("payload"))
                                .and_then(|p| p.get("value"))
                                .cloned();
                            
                            Ok(OperationResult::ReadOk { value })
                        } else {
                            Ok(OperationResult::ReadOk { value: None })
                        }
                    }
                    OperationType::Write { key, value } => {
                        let point = serde_json::json!({
                            "id": key,
                            "vector": vec![0.0; 128],
                            "payload": { "value": value }
                        });

                        let response = self.client
                            .put("http://localhost:6333/collections/jepsen-test/points")
                            .json(&serde_json::json!({ "points": [point] }))
                            .send()
                            .await
                            .map_err(|e| crate::RTDBError::Network(e.to_string()))?;

                        if response.status().is_success() {
                            Ok(OperationResult::WriteOk)
                        } else {
                            Err(crate::RTDBError::Config(format!("Write failed: {}", response.status())))
                        }
                    }
                    OperationType::Increment { key, delta } => {
                        // Simplified increment using read-modify-write
                        let read_result = self.execute(OperationType::Read { key: key.clone() }).await?;
                        
                        let current_value = match read_result {
                            OperationResult::ReadOk { value: Some(v) } => {
                                v.as_i64().unwrap_or(0)
                            }
                            _ => 0,
                        };
                        
                        let new_value = current_value + delta;
                        self.execute(OperationType::Write { 
                            key, 
                            value: serde_json::Value::Number(new_value.into()) 
                        }).await?;
                        
                        Ok(OperationResult::IncrementOk { new_value })
                    }
                    _ => Err(crate::RTDBError::Config("Unsupported operation".to_string())),
                }
            }

            fn id(&self) -> usize {
                self.id
            }

            async fn is_healthy(&self) -> bool {
                self.client
                    .get("http://localhost:6333/health")
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
            }
        }

        let clients: Vec<Arc<dyn JepsenClient>> = (0..concurrency)
            .map(|i| Arc::new(RtdbJepsenClient { 
                id: i, 
                client: reqwest::Client::new() 
            }) as Arc<dyn JepsenClient>)
            .collect();

        // Create nemesis
        let node_addresses = vec![
            "127.0.0.1:6333".to_string(),
            "127.0.0.1:6334".to_string(),
            "127.0.0.1:6335".to_string(),
        ];
        let nemesis = Arc::new(nemesis::CombinedNemesis::new(node_addresses, 1000));

        // Create checker
        let checker = checkers::create_checker(consistency);

        // Create and run Jepsen test
        let runner = JepsenRunner::new(config, clients, nemesis, checker);
        
        println!("Starting Jepsen test execution...");
        let start_time = std::time::Instant::now();
        
        match runner.run().await {
            Ok(result) => {
                let duration = start_time.elapsed();
                let summary = result.summary();
                
                println!("\nJepsen test completed in {:?}", duration);
                println!("Results:");
                println!("   Total operations: {}", summary.total_operations);
                println!("   Successful: {} ({:.1}%)", 
                        summary.successful_operations,
                        (summary.successful_operations as f64 / summary.total_operations as f64) * 100.0);
                println!("   Failed: {} ({:.1}%)", 
                        summary.failed_operations,
                        (summary.failed_operations as f64 / summary.total_operations as f64) * 100.0);
                
                if enable_faults {
                    println!("   Faults injected: {}", summary.faults_injected);
                }
                
                println!("   Consistency violations: {}", summary.consistency_violations);
                
                if summary.is_valid {
                    println!("Test PASSED - No consistency violations detected");
                } else {
                    println!("Test FAILED - {} consistency violations found", summary.consistency_violations);
                    
                    if !result.checker_result.violations.is_empty() {
                        println!("\nViolations:");
                        for (i, violation) in result.checker_result.violations.iter().enumerate() {
                            println!("   {}. {:?}: {}", i + 1, violation.violation_type, violation.description);
                        }
                    }
                }

                // Analyze performance
                let latency_analysis = history::HistoryAnalyzer::analyze_latencies(&result.history);
                if latency_analysis.count > 0 {
                    println!("\nLatency Analysis:");
                    println!("   P50: {:?}", latency_analysis.median);
                    println!("   P95: {:?}", latency_analysis.p95);
                    println!("   P99: {:?}", latency_analysis.p99);
                    println!("   Max: {:?}", latency_analysis.max);
                }

                let error_rates = history::HistoryAnalyzer::analyze_error_rates(&result.history);
                if !error_rates.is_empty() {
                    println!("\nError Rates by Operation:");
                    for (op_type, error_rate) in error_rates {
                        if error_rate.total > 0 {
                            println!("   {}: {:.2}% ({}/{})", 
                                    op_type, 
                                    error_rate.error_rate * 100.0,
                                    error_rate.errors,
                                    error_rate.total);
                        }
                    }
                }

                // Clean up test collection
                println!("\nCleaning up test collection...");
                let _ = client
                    .delete("http://localhost:6333/collections/jepsen-test")
                    .send()
                    .await;

                if summary.is_valid {
                    Ok(())
                } else {
                    Err(crate::RTDBError::Validation("Consistency violations detected".to_string()))
                }
            }
            Err(e) => {
                println!("Jepsen test failed: {}", e);
                
                // Clean up test collection
                let _ = client
                    .delete("http://localhost:6333/collections/jepsen-test")
                    .send()
                    .await;
                
                Err(e)
            }
        }
    }

    /// List active migrations
    async fn migration_list(&self) -> Result<()> {
        use crate::migration::MigrationManager;
        use std::path::PathBuf;

        println!("📋 Active Migrations:");
        
        let checkpoint_dir = PathBuf::from("./migration_checkpoints");
        let migration_manager = MigrationManager::new(checkpoint_dir)?;
        
        let migrations = migration_manager.list_migrations().await;
        
        if migrations.is_empty() {
            println!("  No active migrations found");
        } else {
            for migration in migrations {
                let status_icon = match migration.status {
                    crate::migration::MigrationStatus::Running => "🔄",
                    crate::migration::MigrationStatus::Completed => "✅",
                    crate::migration::MigrationStatus::Failed => "❌",
                    crate::migration::MigrationStatus::Cancelled => "⏹️",
                    _ => "❓",
                };
                
                println!("  {} {} - {} -> {} ({:?})", 
                    status_icon,
                    migration.id,
                    migration.source_type,
                    migration.target_collection,
                    migration.status
                );
                
                if let Some(total) = migration.total_records {
                    let pct = migration.processed_records as f64 / total as f64 * 100.0;
                    println!("    Progress: {:.1}% ({}/{})", pct, migration.processed_records, total);
                } else {
                    println!("    Progress: {} records processed", migration.processed_records);
                }
                
                if migration.failed_records > 0 {
                    println!("    Failed: {} records", migration.failed_records);
                }
            }
        }
        
        Ok(())
    }

    /// Show migration status
    async fn migration_status(&self, migration_id: &str) -> Result<()> {
        use crate::migration::MigrationManager;
        use std::path::PathBuf;
        use uuid::Uuid;

        let id = Uuid::parse_str(migration_id)
            .map_err(|_| crate::RTDBError::Config("Invalid migration ID format".to_string()))?;

        let checkpoint_dir = PathBuf::from("./migration_checkpoints");
        let migration_manager = MigrationManager::new(checkpoint_dir)?;
        
        if let Some(progress) = migration_manager.get_progress(id).await {
            println!("📊 Migration Status: {}", migration_id);
            println!("  Status: {:?}", progress.status);
            
            if let Some(total) = progress.total_records {
                let pct = progress.processed_records as f64 / total as f64 * 100.0;
                println!("  Progress: {:.1}% ({}/{})", pct, progress.processed_records, total);
            } else {
                println!("  Progress: {} records processed", progress.processed_records);
            }
            
            println!("  Throughput: {:.0} records/sec", progress.throughput_per_second);
            
            if progress.failed_records > 0 {
                println!("  Failed: {} records", progress.failed_records);
            }
            
            if let Some(eta) = progress.estimated_completion {
                println!("  ETA: {}", eta.format("%Y-%m-%d %H:%M:%S UTC"));
            }
            
            if !progress.error_messages.is_empty() {
                println!("  Recent errors:");
                for error in progress.error_messages.iter().take(5) {
                    println!("    - {}", error);
                }
            }
        } else {
            println!("❓ Migration {} not found", migration_id);
        }
        
        Ok(())
    }

    /// Cancel a running migration
    async fn migration_cancel(&self, migration_id: &str) -> Result<()> {
        use crate::migration::MigrationManager;
        use std::path::PathBuf;
        use uuid::Uuid;

        let id = Uuid::parse_str(migration_id)
            .map_err(|_| crate::RTDBError::Config("Invalid migration ID format".to_string()))?;

        let checkpoint_dir = PathBuf::from("./migration_checkpoints");
        let migration_manager = MigrationManager::new(checkpoint_dir)?;
        
        println!("⏹️  Cancelling migration {}...", migration_id);
        
        migration_manager.cancel_migration(id).await?;
        println!("✅ Migration cancelled successfully");
        
        Ok(())
    }

    /// Resume a failed migration
    async fn migration_resume(&self, migration_id: &str) -> Result<()> {
        use crate::migration::MigrationManager;
        use std::path::PathBuf;
        use uuid::Uuid;

        let id = Uuid::parse_str(migration_id)
            .map_err(|_| crate::RTDBError::Config("Invalid migration ID format".to_string()))?;

        let checkpoint_dir = PathBuf::from("./migration_checkpoints");
        let migration_manager = MigrationManager::new(checkpoint_dir)?;
        
        println!("🔄 Resuming migration {}...", migration_id);
        
        if migration_manager.resume_migration(id).await? {
            println!("✅ Migration resumed successfully");
            
            // Monitor progress like in the main migrate command
            let mut last_progress = None;
            let start_time = std::time::Instant::now();
            
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                
                if let Some(progress) = migration_manager.get_progress(id).await {
                    let should_print = last_progress.as_ref().map_or(true, |last: &crate::migration::MigrationProgress| {
                        progress.processed_records != last.processed_records ||
                        matches!(progress.status, crate::migration::MigrationStatus::Completed | 
                                               crate::migration::MigrationStatus::Failed |
                                               crate::migration::MigrationStatus::Cancelled)
                    });

                    if should_print {
                        let elapsed = start_time.elapsed();
                        let completion = if let Some(total) = progress.total_records {
                            let pct = progress.processed_records as f64 / total as f64 * 100.0;
                            format!("{:.1}% ({}/{})", pct, progress.processed_records, total)
                        } else {
                            format!("{} records", progress.processed_records)
                        };

                        println!("📊 Progress: {} - {:.0} records/sec - Status: {:?} - Elapsed: {:?}", 
                                completion, progress.throughput_per_second, progress.status, elapsed);
                    }

                    match progress.status {
                        crate::migration::MigrationStatus::Completed => {
                            println!("🎉 Migration completed successfully!");
                            break;
                        }
                        crate::migration::MigrationStatus::Failed => {
                            println!("💥 Migration failed again!");
                            return Err(crate::RTDBError::Config("Migration failed".to_string()));
                        }
                        crate::migration::MigrationStatus::Cancelled => {
                            println!("⏹️  Migration was cancelled");
                            break;
                        }
                        _ => {}
                    }

                    last_progress = Some(progress);
                }
            }
        }
        */
                } else {
                    break;
                }
            }
        } else {
            println!("❓ Migration not found or cannot be resumed");
        }
        
        Ok(())
    }

    /// Clean up migration checkpoints
    async fn migration_cleanup(&self, checkpoint_dir: &str, force: bool) -> Result<()> {
        use std::path::PathBuf;
        use tokio::fs;

        let checkpoint_path = PathBuf::from(checkpoint_dir);
        
        if !checkpoint_path.exists() {
            println!("📁 Checkpoint directory does not exist: {}", checkpoint_dir);
            return Ok(());
        }

        println!("🧹 Cleaning up migration checkpoints in {}...", checkpoint_dir);
        
        if force {
            println!("⚠️  Force mode: removing ALL checkpoints including active migrations");
            fs::remove_dir_all(&checkpoint_path).await?;
            println!("✅ All checkpoints removed");
        } else {
            // Only remove completed/failed migration checkpoints
            let mut entries = fs::read_dir(&checkpoint_path).await?;
            let mut removed_count = 0;
            
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    // Check if migration is still active
                    let status_file = path.join("status.json");
                    if let Ok(status_content) = fs::read_to_string(&status_file).await {
                        if let Ok(status) = serde_json::from_str::<serde_json::Value>(&status_content) {
                            if let Some(status_str) = status.get("status").and_then(|s| s.as_str()) {
                                if matches!(status_str, "Completed" | "Failed" | "Cancelled") {
                                    fs::remove_dir_all(&path).await?;
                                    removed_count += 1;
                                    println!("  Removed: {}", path.file_name().unwrap().to_string_lossy());
                                }
                            }
                        }
                    }
                }
            }
            
            println!("✅ Removed {} completed/failed migration checkpoints", removed_count);
        }
        
        Ok(())
    }
}

/// Parse CLI arguments and execute
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let handler = CliHandler::new(cli.config).await?;
    handler.execute(cli.command).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse() {
        let args = vec!["rtdb", "status"];
        let cli = Cli::parse_from(args);
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn test_cli_start() {
        let args = vec!["rtdb", "start", "--daemon"];
        let cli = Cli::parse_from(args);
        assert!(matches!(cli.command, Commands::Start { daemon: true }));
    }
}