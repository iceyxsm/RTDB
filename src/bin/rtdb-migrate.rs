//! RTDB Migration Tool - High-Performance Vector Database Migration
//!
//! A production-grade migration tool with SIMD optimizations for migrating
//! between vector databases with maximum performance and reliability.

use clap::Parser;
use rtdb::migration::cli::MigrationCli;
use std::process;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("RTDB Migration Tool v1.0.0 - SIMD-Optimized Vector Database Migration");

    // Parse command line arguments
    let cli = MigrationCli::parse();

    // Execute migration
    match cli.execute().await {
        Ok(()) => {
            info!("Migration completed successfully!");
            process::exit(0);
        }
        Err(e) => {
            error!("Migration failed: {}", e);
            process::exit(1);
        }
    }
}