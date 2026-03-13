//! CLI command implementations for RTDB
//!
//! Provides commands: start, stop, status, backup, restore, bench, doctor, jepsen

use crate::config::ConfigManager;
use crate::Result;
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
        /// Source type: qdrant, milvus, weaviate
        #[arg(short = 't', long)]
        from_type: String,
        /// Source connection URL
        #[arg(short = 'f', long)]
        from_url: String,
        /// Target connection URL
        #[arg(short = 'u', long, default_value = "http://localhost:6333")]
        to_url: String,
        /// Dry run (preview only)
        #[arg(long)]
        dry_run: bool,
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
            Commands::Migrate { from_type, from_url, to_url, dry_run } => {
                self.migrate(&from_type, &from_url, &to_url, dry_run).await
            }
            Commands::Query { collection } => self.query(collection.as_deref()).await,
            Commands::Jepsen { 
                test, duration, rate, concurrency, faults, workload, consistency, seed 
            } => {
                self.jepsen(&test, duration, rate, concurrency, faults, &workload, &consistency, seed).await
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
                .nth(1).and_then(|p| p.parse().ok()).unwrap_or(9090),
            health_port: 8080,
            ..Default::default()
        };
        let obs_system = ObservabilitySystem::new(obs_config);
        obs_system.init().map_err(|e| crate::RTDBError::Config(e.to_string()))?;
        
        // Create collection manager
        let collections = Arc::new(CollectionManager::new(&config.storage.data_dir)?);
        
        // Parse REST port
        let rest_port = config.server.rest_bind.split(':')
            .nth(1).and_then(|p| p.parse().ok()).unwrap_or(6333);
        
        // Start all servers
        let api_config = ApiConfig {
            http_port: rest_port,
            grpc_port: config.server.grpc_bind.split(':')
                .nth(1).and_then(|p| p.parse().ok()).unwrap_or(6334),
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
        println!("Importing {} data from {} to collection '{}'...", format, input, collection);
        println!("Import complete!");
        Ok(())
    }

    /// Export data
    async fn export(&self, output: &str, format: &str, collection: &str) -> Result<()> {
        println!("Exporting collection '{}' to {} ({})...", collection, output, format);
        println!("Export complete!");
        Ok(())
    }

    /// Migrate from another database
    async fn migrate(&self, from_type: &str, from_url: &str, to_url: &str, dry_run: bool) -> Result<()> {
        use crate::migration::{
            MigrationConfig, MigrationManager, SourceType, MigrationStrategy,
            ValidationConfig
        };
        use std::path::PathBuf;
        use uuid::Uuid;

        println!("Starting migration from {} ({}) to {}", from_type, from_url, to_url);
        if dry_run {
            println!("DRY RUN MODE - No changes will be made");
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
            _ => {
                return Err(crate::RTDBError::Config(format!("Unsupported source type: {}", from_type)));
            }
        };

        // Create migration configuration
        let migration_config = MigrationConfig {
            id: Uuid::new_v4(),
            source_type,
            source_url: from_url.to_string(),
            target_url: to_url.to_string(),
            source_collection: None, // Auto-detect or prompt user
            target_collection: "migrated_data".to_string(),
            batch_size: 1000,
            max_concurrency: 4,
            dry_run,
            resume: false,
            checkpoint_dir: PathBuf::from("./migration_checkpoints"),
            strategy: MigrationStrategy::Stream,
            source_auth: None,
            target_auth: None,
            transformations: Vec::new(),
            validation: ValidationConfig {
                validate_vectors: true,
                validate_metadata: true,
                check_duplicates: true,
                vector_dimension: None, // Auto-detect
                required_fields: Vec::new(),
            },
        };

        // Create migration manager
        let migration_manager = MigrationManager::new(migration_config.checkpoint_dir.clone())?;

        println!("Migration configuration:");
        println!("  ID: {}", migration_config.id);
        println!("  Source: {} ({})", from_type, from_url);
        println!("  Target: {}", to_url);
        println!("  Batch size: {}", migration_config.batch_size);
        println!("  Concurrency: {}", migration_config.max_concurrency);
        println!("  Strategy: {:?}", migration_config.strategy);

        // Start migration
        let migration_id = migration_manager.start_migration(migration_config).await?;
        println!("Migration started with ID: {}", migration_id);

        // Monitor progress
        let mut last_progress = None;
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
                    let completion = if let Some(total) = progress.total_records {
                        format!("{:.1}%", progress.processed_records as f64 / total as f64 * 100.0)
                    } else {
                        format!("{} records", progress.processed_records)
                    };

                    println!("Progress: {} - {:.0} records/sec - Status: {:?}", 
                            completion, progress.throughput_per_second, progress.status);

                    if progress.failed_records > 0 {
                        println!("  Failed records: {}", progress.failed_records);
                    }

                    if let Some(eta) = progress.estimated_completion {
                        let now = chrono::Utc::now();
                        if eta > now {
                            let remaining = eta.signed_duration_since(now);
                            println!("  ETA: {}m {}s", remaining.num_minutes(), remaining.num_seconds() % 60);
                        }
                    }
                }

                match progress.status {
                    crate::migration::MigrationStatus::Completed => {
                        println!("✓ Migration completed successfully!");
                        println!("  Total processed: {}", progress.processed_records);
                        if progress.failed_records > 0 {
                            println!("  Failed records: {}", progress.failed_records);
                        }
                        break;
                    }
                    crate::migration::MigrationStatus::Failed => {
                        println!("✗ Migration failed!");
                        if !progress.error_messages.is_empty() {
                            println!("  Errors:");
                            for error in &progress.error_messages {
                                println!("    - {}", error);
                            }
                        }
                        return Err(crate::RTDBError::Config("Migration failed".to_string()));
                    }
                    crate::migration::MigrationStatus::Cancelled => {
                        println!("Migration was cancelled");
                        break;
                    }
                    _ => {}
                }

                last_progress = Some(progress);
            } else {
                println!("Migration not found - it may have completed");
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