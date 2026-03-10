//! RTDB - Production-Grade Smart Vector Database
//!
//! Main entry point for the RTDB server and CLI

use rtdb::cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run CLI
    cli::run().await.map_err(|e| e.into())
}
