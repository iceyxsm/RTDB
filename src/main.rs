//! RTDB - Production-Grade Smart Vector Database
//!
//! Main entry point for the RTDB server and CLI

use rtdb::cli;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize SIMDX context for optimal performance
    let simdx_context = rtdb::simdx::initialize_simdx();
    let stats = simdx_context.get_performance_stats();
    info!("SIMDX initialized: backend={:?}, performance_boost={:.1}x, vector_width={}bits", 
          stats.backend, stats.performance_multiplier, stats.vector_width);
    
    // Run CLI
    cli::run().await.map_err(|e| e.into())
}
