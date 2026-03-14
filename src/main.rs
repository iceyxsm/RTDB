//! RTDB - Production-Grade Smart Vector Database
//!
//! Main entry point for the RTDB server and CLI

use rtdb::cli;
use rtdb::simdx::SIMDXEngine;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize SIMDX engine for optimal performance
    let simdx_engine = SIMDXEngine::new(None);
    let capabilities = simdx_engine.get_capabilities();
    let metrics = simdx_engine.get_metrics();
    
    info!("SIMDX initialized: backend={:?}, vector_width={}, operations={}", 
          capabilities.preferred_backend, 
          capabilities.vector_width,
          metrics.operations_count);
    
    // Run CLI
    cli::run().await.map_err(|e| e.into())
}
