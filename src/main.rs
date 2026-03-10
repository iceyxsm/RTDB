//! RTDB - Production-Grade Smart Vector Database
//! 
//! Usage:
//!   rtdb serve          Start the database server
//!   rtdb status         Check server status
//!   rtdb migrate        Migrate from another database
//!   rtdb backup         Create backup
//!   rtdb restore        Restore from backup

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "rtdb")]
#[command(about = "RTDB - Smart Vector Database")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the database server
    Serve {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Storage directory
        #[arg(short, long, default_value = "./rtdb_storage")]
        storage: String,
        
        /// REST API port
        #[arg(long, default_value = "6333")]
        http_port: u16,
        
        /// gRPC port
        #[arg(long, default_value = "6334")]
        grpc_port: u16,
    },
    
    /// Check server health
    Status {
        /// Server URL
        #[arg(short, long, default_value = "http://localhost:6333")]
        url: String,
    },
    
    /// Migrate from another database
    Migrate {
        /// Source database type (qdrant, milvus, weaviate, lancedb)
        #[arg(short, long)]
        from_type: String,
        
        /// Source database URL
        #[arg(short, long)]
        from_url: String,
        
        /// Target RTDB URL
        #[arg(short, long, default_value = "http://localhost:6333")]
        to_url: String,
        
        /// Dry run (preview changes)
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Create backup
    Backup {
        /// Backup destination path
        #[arg(short, long)]
        output: String,
        
        /// Server URL
        #[arg(short, long, default_value = "http://localhost:6333")]
        url: String,
    },
    
    /// Restore from backup
    Restore {
        /// Backup source path
        #[arg(short, long)]
        input: String,
        
        /// Server URL
        #[arg(short, long, default_value = "http://localhost:6333")]
        url: String,
    },
    
    /// Run benchmarks
    Bench {
        /// Benchmark type
        #[arg(short, long, default_value = "latency")]
        benchmark: String,
        
        /// Number of vectors
        #[arg(short, long, default_value = "100000")]
        vectors: usize,
        
        /// Vector dimension
        #[arg(long, default_value = "768")]
        dimension: usize,
    },
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rtdb=info".into())
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config: _, storage, http_port, grpc_port } => {
            info!("Starting RTDB server");
            info!("Storage: {}", storage);
            info!("HTTP port: {}", http_port);
            info!("gRPC port: {}", grpc_port);

            // Initialize collection manager
            let collections = match rtdb::collection::CollectionManager::new(&storage) {
                Ok(cm) => Arc::new(cm),
                Err(e) => {
                    eprintln!("Failed to initialize collection manager: {}", e);
                    std::process::exit(1);
                }
            };

            info!("Loaded {} collections", collections.list_collections().len());

            // Start REST server
            let rest_collections = collections.clone();
            let rest_handle = tokio::spawn(async move {
                if let Err(e) = rtdb::api::rest::start_server(http_port, rest_collections).await {
                    eprintln!("REST server error: {}", e);
                }
            });

            info!("REST server listening on port {}", http_port);

            // Start gRPC server if enabled
            #[cfg(grpc_enabled)]
            {
                let grpc_collections = collections.clone();
                let grpc_handle = tokio::spawn(async move {
                    if let Err(e) = rtdb::api::grpc::start_server(grpc_port, grpc_collections).await {
                        eprintln!("gRPC server error: {}", e);
                    }
                });

                info!("gRPC server listening on port {}", grpc_port);

                // Wait for both servers
                tokio::select! {
                    _ = rest_handle => {},
                    _ = grpc_handle => {},
                }
            }
            
            #[cfg(not(grpc_enabled))]
            {
                info!("gRPC not enabled (protoc not found)");
                if let Err(e) = rest_handle.await {
                    eprintln!("Server error: {}", e);
                }
            }
        }
        
        Commands::Status { url } => {
            info!("Checking status at {}", url);
            // TODO: Implement status check
            println!("Server status check not yet implemented");
        }
        
        Commands::Migrate { from_type, from_url, to_url, dry_run } => {
            info!("Migrating from {} ({}) to {}", from_type, from_url, to_url);
            if dry_run {
                info!("DRY RUN MODE");
            }
            println!("Migration not yet implemented");
        }
        
        Commands::Backup { output, url } => {
            info!("Creating backup at {} from {}", output, url);
            println!("Backup not yet implemented");
        }
        
        Commands::Restore { input, url } => {
            info!("Restoring from {} to {}", input, url);
            println!("Restore not yet implemented");
        }
        
        Commands::Bench { benchmark, vectors, dimension } => {
            info!("Running {} benchmark with {} vectors (dim={})", 
                benchmark, vectors, dimension);
            println!("Benchmarks not yet implemented");
        }
    }
}
