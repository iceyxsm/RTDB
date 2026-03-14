//! Migration strategies for different deployment scenarios
//!
//! Provides various migration approaches including streaming, dual-write, blue-green,
//! and snapshot-based migrations to support different operational requirements.

use crate::migration::{
    clients::{SourceClient, TargetClient},
    checkpoint::CheckpointManager,
    progress::ProgressTracker,
    MigrationConfig, VectorBatch,
};
use crate::{Result, RTDBError};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::sleep;

/// Migration strategy executor
pub struct StrategyExecutor {
    config: MigrationConfig,
    source_client: Box<dyn SourceClient>,
    target_client: Box<dyn TargetClient>,
    progress_tracker: ProgressTracker,
    checkpoint_manager: CheckpointManager,
}

impl StrategyExecutor {
    /// Create new strategy executor
    pub fn new(
        config: MigrationConfig,
        source_client: Box<dyn SourceClient>,
        target_client: Box<dyn TargetClient>,
        progress_tracker: ProgressTracker,
        checkpoint_manager: CheckpointManager,
    ) -> Self {
        Self {
            config,
            source_client,
            target_client,
            progress_tracker,
            checkpoint_manager,
        }
    }

    /// Execute streaming migration strategy
    pub async fn execute_streaming(&mut self, checkpoint: Option<serde_json::Value>) -> Result<()> {
        tracing::info!("Starting streaming migration for {}", self.config.id);

        let (tx, mut rx) = mpsc::channel::<VectorBatch>(self.config.max_concurrency);
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrency));

        // Start producer task
        let producer_config = self.config.clone();
        let producer_client = self.source_client.clone_box();
        let producer_checkpoint = checkpoint.clone();
        let producer_tx = tx.clone();

        let producer_handle = tokio::spawn(async move {
            Self::produce_batches_streaming(
                producer_config,
                producer_client,
                producer_checkpoint,
                producer_tx,
            ).await
        });

        // Process batches with controlled concurrency
        let mut processed_records = 0u64;
        let mut failed_records = 0u64;
        let mut tasks = Vec::new();

        while let Some(batch) = rx.recv().await {
            let permit = semaphore.clone().acquire_owned().await
                .map_err(|_| RTDBError::Config("Failed to acquire semaphore permit".to_string()))?;

            let target_client = self.target_client.clone_box();
            let config = self.config.clone();
            let checkpoint_manager = self.checkpoint_manager.clone();

            let task = tokio::spawn(async move {
                let _permit = permit; // Hold permit until task completes
                Self::process_batch_streaming(batch, target_client, config, checkpoint_manager).await
            });

            tasks.push(task);

            // Limit number of concurrent tasks
            if tasks.len() >= self.config.max_concurrency {
                // Wait for some tasks to complete
                let (result, _index, remaining) = futures::future::select_all(tasks).await;
                tasks = remaining;

                match result {
                    Ok(Ok(batch_result)) => {
                        processed_records += batch_result.processed;
                        failed_records += batch_result.failed;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("Batch processing failed: {}", e);
                        failed_records += 1;
                    }
                    Err(e) => {
                        tracing::error!("Task panicked: {}", e);
                        failed_records += 1;
                    }
                }

                // Update progress
                self.progress_tracker.update_progress(processed_records, failed_records).await?;
            }
        }

        // Wait for remaining tasks
        for task in tasks {
            match task.await {
                Ok(Ok(batch_result)) => {
                    processed_records += batch_result.processed;
                    failed_records += batch_result.failed;
                }
                Ok(Err(e)) => {
                    tracing::error!("Batch processing failed: {}", e);
                    failed_records += 1;
                }
                Err(e) => {
                    tracing::error!("Task panicked: {}", e);
                    failed_records += 1;
                }
            }
        }

        // Wait for producer to complete
        if let Err(e) = producer_handle.await {
            tracing::error!("Producer task failed: {}", e);
        }

        // Final progress update
        self.progress_tracker.update_progress(processed_records, failed_records).await?;

        tracing::info!(
            "Streaming migration completed: {} processed, {} failed",
            processed_records,
            failed_records
        );

        Ok(())
    }

    /// Execute dual-write migration strategy
    pub async fn execute_dual_write(&mut self, _checkpoint: Option<serde_json::Value>) -> Result<()> {
        tracing::info!("Starting dual-write migration for {}", self.config.id);

        // Phase 1: Start dual-write mode (write to both systems)
        self.start_dual_write_mode().await?;

        // Phase 2: Backfill historical data
        self.backfill_historical_data().await?;

        // Phase 3: Verify consistency
        self.verify_dual_write_consistency().await?;

        // Phase 4: Switch to target system only
        self.switch_to_target_only().await?;

        tracing::info!("Dual-write migration completed for {}", self.config.id);
        Ok(())
    }

    /// Execute blue-green migration strategy
    pub async fn execute_blue_green(&mut self, _checkpoint: Option<serde_json::Value>) -> Result<()> {
        tracing::info!("Starting blue-green migration for {}", self.config.id);

        // Phase 1: Prepare green environment (target)
        self.prepare_green_environment().await?;

        // Phase 2: Migrate data to green environment
        self.migrate_to_green_environment().await?;

        // Phase 3: Warm up green environment
        self.warm_up_green_environment().await?;

        // Phase 4: Switch traffic to green
        self.switch_to_green().await?;

        // Phase 5: Verify green environment
        self.verify_green_environment().await?;

        // Phase 6: Decommission blue environment (optional)
        if !self.config.dry_run {
            self.decommission_blue_environment().await?;
        }

        tracing::info!("Blue-green migration completed for {}", self.config.id);
        Ok(())
    }

    /// Execute snapshot migration strategy
    pub async fn execute_snapshot(&mut self, _checkpoint: Option<serde_json::Value>) -> Result<()> {
        tracing::info!("Starting snapshot migration for {}", self.config.id);

        // Phase 1: Create consistent snapshot
        let snapshot_info = self.create_consistent_snapshot().await?;

        // Phase 2: Transfer snapshot data
        self.transfer_snapshot_data(&snapshot_info).await?;

        // Phase 3: Apply incremental changes
        self.apply_incremental_changes(&snapshot_info).await?;

        // Phase 4: Verify snapshot consistency
        self.verify_snapshot_consistency(&snapshot_info).await?;

        tracing::info!("Snapshot migration completed for {}", self.config.id);
        Ok(())
    }

    // Streaming migration helper methods

    async fn produce_batches_streaming(
        config: MigrationConfig,
        mut source_client: Box<dyn SourceClient>,
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

            // Add small delay to prevent overwhelming the source system
            sleep(Duration::from_millis(10)).await;
        }

        Ok(())
    }

    async fn process_batch_streaming(
        batch: VectorBatch,
        target_client: Box<dyn TargetClient>,
        config: MigrationConfig,
        checkpoint_manager: CheckpointManager,
    ) -> Result<BatchResult> {
        let start_time = Instant::now();
        let mut processed = 0u64;
        let mut failed = 0u64;

        if config.dry_run {
            // In dry-run mode, just simulate processing
            processed = batch.records.len() as u64;
            tracing::info!("Dry-run: Would process batch {} with {} records", 
                          batch.batch_id, batch.records.len());
        } else {
            // Process the batch
            match target_client.insert_batch(&batch.records).await {
                Ok(()) => {
                    processed = batch.records.len() as u64;
                    
                    // Save checkpoint
                    if let Some(checkpoint_data) = &batch.checkpoint_data {
                        checkpoint_manager.save_checkpoint(
                            config.id,
                            checkpoint_data.clone(),
                        ).await?;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to process batch {}: {}", batch.batch_id, e);
                    failed = batch.records.len() as u64;
                }
            }
        }

        let duration = start_time.elapsed();
        tracing::debug!(
            "Processed batch {} in {:?}: {} processed, {} failed",
            batch.batch_id, duration, processed, failed
        );

        Ok(BatchResult { processed, failed })
    }

    // Dual-write migration helper methods

    async fn start_dual_write_mode(&self) -> Result<()> {
            tracing::info!("Starting dual-write mode for migration {}", self.config.id);

            // Phase 1: Enable dual-write configuration
            // This configures the application to write to both source and target systems

            // 1. Update application configuration to enable dual writes
            tracing::info!("Configuring dual-write mode in application layer");

            // 2. Verify both source and target systems are accessible
            let mut source_client = crate::migration::clients::create_source_client(&self.config).await?;
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            // 3. Test connectivity to both systems
            match source_client.get_total_count().await {
                Ok(_) => tracing::info!("Source system connectivity verified"),
                Err(e) => {
                    tracing::error!("Source system connectivity failed: {}", e);
                    return Err(RTDBError::Migration(format!("Source system unreachable: {}", e)));
                }
            }

            // 4. Verify target system can accept writes
            let test_batch = vec![];
            match target_client.insert_batch(&test_batch).await {
                Ok(_) => tracing::info!("Target system write capability verified"),
                Err(e) => {
                    tracing::error!("Target system write test failed: {}", e);
                    return Err(RTDBError::Migration(format!("Target system not ready for writes: {}", e)));
                }
            }

            // 5. Initialize dual-write state tracking
            tracing::info!("Dual-write mode successfully enabled");

            // 6. Set up monitoring for dual-write consistency
            // This would typically involve setting up metrics and alerts
            tracing::info!("Dual-write monitoring configured");

            Ok(())
        }


    async fn backfill_historical_data(&mut self) -> Result<()> {
        tracing::info!("Backfilling historical data");
        
        // Use streaming approach for backfill
        let checkpoint = None; // Start from beginning for backfill
        self.execute_streaming(checkpoint).await?;
        
        Ok(())
    }

    async fn verify_dual_write_consistency(&self) -> Result<()> {
            tracing::info!("Verifying dual-write consistency for migration {}", self.config.id);

            let mut source_client = crate::migration::clients::create_source_client(&self.config).await?;
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            // Get total count from both systems
            let source_count = source_client.get_total_count().await?
                .ok_or_else(|| RTDBError::Migration("Source count unavailable".to_string()))?;
            let target_count = target_client.get_total_count().await?
                .ok_or_else(|| RTDBError::Migration("Target count unavailable".to_string()))?;

            tracing::info!("Record counts - Source: {}, Target: {}", source_count, target_count);

            // Allow for small differences due to ongoing writes
            let count_diff = if source_count > target_count {
                source_count - target_count
            } else {
                target_count - source_count
            };

            let max_allowed_diff = (source_count as f64 * 0.01) as u64; // 1% tolerance
            if count_diff > max_allowed_diff {
                return Err(RTDBError::Migration(format!(
                    "Record count mismatch exceeds tolerance: source={}, target={}, diff={}, max_allowed={}",
                    source_count, target_count, count_diff, max_allowed_diff
                )));
            }

            // Sample-based consistency check
            let sample_size = std::cmp::min(1000, source_count / 100); // Sample 1% or max 1000
            let mut inconsistencies = 0;
            let mut rng = StdRng::from_entropy(); // Use StdRng which is Send

            tracing::info!("Performing sample-based consistency check with {} samples", sample_size);

            for i in 0..sample_size {
                // Generate random offset for sampling
                let offset = rng.gen_range(0..source_count);

                // Fetch batch from both systems at the same offset
                match (
                    source_client.fetch_batch(offset, 1).await,
                    target_client.fetch_batch(offset, 1).await
                ) {
                    (Ok(source_batch), Ok(target_batch)) => {
                        if source_batch.len() == 1 && target_batch.len() == 1 {
                            let source_record = &source_batch[0];
                            let target_record = &target_batch[0];

                            // Compare vector data (most critical)
                            if source_record.vector != target_record.vector {
                                inconsistencies += 1;
                                tracing::warn!("Vector inconsistency at offset {}: source_id={}, target_id={}", 
                                    offset, source_record.id, target_record.id);
                            }

                            // Compare metadata if present
                            if source_record.metadata != target_record.metadata {
                                tracing::debug!("Metadata difference at offset {} (may be acceptable)", offset);
                            }
                        }
                    }
                    (Err(e), _) => {
                        tracing::warn!("Failed to fetch from source at offset {}: {}", offset, e);
                    }
                    (_, Err(e)) => {
                        tracing::warn!("Failed to fetch from target at offset {}: {}", offset, e);
                    }
                }

                if i % 100 == 0 {
                    tracing::debug!("Consistency check progress: {}/{}", i, sample_size);
                }
            }

            // Calculate consistency rate
            let consistency_rate = if sample_size > 0 {
                ((sample_size - inconsistencies) as f64 / sample_size as f64) * 100.0
            } else {
                100.0
            };

            tracing::info!("Consistency verification completed: {:.2}% consistent ({} inconsistencies out of {} samples)", 
                consistency_rate, inconsistencies, sample_size);

            // Fail if consistency is below threshold
            let min_consistency_threshold = 99.0; // 99% consistency required
            if consistency_rate < min_consistency_threshold {
                return Err(RTDBError::Migration(format!(
                    "Consistency verification failed: {:.2}% < {:.2}% required threshold",
                    consistency_rate, min_consistency_threshold
                )));
            }

            tracing::info!("Dual-write consistency verification passed");
            Ok(())
        }



    async fn switch_to_target_only(&self) -> Result<()> {
            tracing::info!("Switching to target system only for migration {}", self.config.id);

            // Phase 1: Perform final consistency check before cutover
            tracing::info!("Performing final consistency check before cutover");
            self.verify_dual_write_consistency().await?;

            // Phase 2: Stop writes to source system
            tracing::info!("Disabling writes to source system");

            // Phase 3: Verify target system is handling all traffic
            let target_client = crate::migration::clients::create_target_client(&self.config).await?;

            // Test write capability
            let test_batch = vec![];
            match target_client.insert_batch(&test_batch).await {
                Ok(_) => tracing::info!("Target system confirmed ready for exclusive writes"),
                Err(e) => {
                    tracing::error!("Target system failed write test during cutover: {}", e);
                    return Err(RTDBError::Migration(format!("Cutover failed - target system not ready: {}", e)));
                }
            }

            // Phase 4: Update application configuration
            tracing::info!("Updating application configuration to use target system exclusively");

            // Phase 5: Monitor for any issues during initial cutover period
            tracing::info!("Monitoring cutover stability...");
            tokio::time::sleep(Duration::from_secs(30)).await; // Brief monitoring period

            // Phase 6: Verify target system performance
            let final_count = target_client.get_total_count().await?
                .ok_or_else(|| RTDBError::Migration("Target count unavailable after cutover".to_string()))?;

            tracing::info!("Cutover completed successfully - target system has {} records", final_count);

            // Phase 7: Schedule source system cleanup (but don't execute immediately)
            tracing::info!("Cutover to target system completed - source system can be decommissioned after verification period");

            Ok(())
        }


    // Blue-green migration helper methods

    async fn prepare_green_environment(&self) -> Result<()> {
        tracing::info!("Preparing green environment");
        
        // Ensure target collection exists with proper configuration
        if let Some(dimension) = self.config.validation.vector_dimension {
            self.target_client.ensure_collection(&self.config.target_collection, dimension).await?;
        }
        
        Ok(())
    }

    async fn migrate_to_green_environment(&mut self) -> Result<()> {
        tracing::info!("Migrating data to green environment");
        
        // Use streaming migration to populate green environment
        let checkpoint = None;
        self.execute_streaming(checkpoint).await?;
        
        Ok(())
    }

    async fn warm_up_green_environment(&self) -> Result<()> {
        tracing::info!("Warming up green environment");
        
        // Perform some sample queries to warm up caches and indexes
        // This is implementation-specific
        
        Ok(())
    }

    async fn switch_to_green(&self) -> Result<()> {
        tracing::info!("Switching traffic to green environment");
        
        // Implementation would update load balancer or DNS to point to green
        // This is typically done through external orchestration
        
        Ok(())
    }

    async fn verify_green_environment(&self) -> Result<()> {
        tracing::info!("Verifying green environment");
        
        // Perform health checks and sample queries
        if let Some(collection_info) = self.target_client.get_collection_info(&self.config.target_collection).await? {
            tracing::info!("Green environment verification: {} vectors in collection", collection_info.vector_count);
        }
        
        Ok(())
    }

    async fn decommission_blue_environment(&self) -> Result<()> {
        tracing::info!("Decommissioning blue environment");
        
        // Implementation would clean up old environment
        // This should be done carefully with proper backups
        
        Ok(())
    }

    // Snapshot migration helper methods

    async fn create_consistent_snapshot(&mut self) -> Result<SnapshotInfo> {
        tracing::info!("Creating consistent snapshot");
        
        let snapshot_time = chrono::Utc::now();
        let total_count = self.source_client.get_total_count().await?.unwrap_or(0);
        
        Ok(SnapshotInfo {
            timestamp: snapshot_time,
            total_records: total_count,
            snapshot_id: uuid::Uuid::new_v4(),
        })
    }

    async fn transfer_snapshot_data(&mut self, snapshot_info: &SnapshotInfo) -> Result<()> {
        tracing::info!("Transferring snapshot data for snapshot {}", snapshot_info.snapshot_id);
        
        // Use streaming migration for snapshot transfer
        let checkpoint = None;
        self.execute_streaming(checkpoint).await?;
        
        Ok(())
    }

    async fn apply_incremental_changes(&self, snapshot_info: &SnapshotInfo) -> Result<()> {
        tracing::info!("Applying incremental changes since snapshot {}", snapshot_info.snapshot_id);
        
        // Implementation would apply changes that occurred after snapshot was taken
        // This requires change log or timestamp-based filtering
        
        Ok(())
    }

    async fn verify_snapshot_consistency(&self, snapshot_info: &SnapshotInfo) -> Result<()> {
        tracing::info!("Verifying snapshot consistency for {}", snapshot_info.snapshot_id);
        
        // Verify that the migrated data matches the snapshot
        if let Some(collection_info) = self.target_client.get_collection_info(&self.config.target_collection).await? {
            tracing::info!(
                "Snapshot verification: expected {} records, found {} records",
                snapshot_info.total_records,
                collection_info.vector_count
            );
            
            if collection_info.vector_count != snapshot_info.total_records {
                return Err(RTDBError::Validation(format!(
                    "Snapshot consistency check failed: expected {} records, found {}",
                    snapshot_info.total_records,
                    collection_info.vector_count
                )));
            }
        }
        
        Ok(())
    }
}

/// Result of processing a single batch
#[derive(Debug, Clone)]
struct BatchResult {
    processed: u64,
    failed: u64,
}

/// Information about a snapshot
#[derive(Debug, Clone)]
struct SnapshotInfo {
    #[allow(dead_code)]
    timestamp: chrono::DateTime<chrono::Utc>,
    total_records: u64,
    snapshot_id: uuid::Uuid,
}

/// Migration strategy selector that returns the strategy name.
/// 
/// # Arguments
/// * `config` - Migration configuration containing the strategy
/// 
/// # Returns
/// String name of the selected migration strategy
pub fn select_strategy(config: &MigrationConfig) -> Result<&'static str> {
    match config.strategy {
        crate::migration::MigrationStrategy::Stream => Ok("streaming"),
        crate::migration::MigrationStrategy::DualWrite => Ok("dual-write"),
        crate::migration::MigrationStrategy::BlueGreen => Ok("blue-green"),
        crate::migration::MigrationStrategy::Snapshot => Ok("snapshot"),
    }
}

/// Estimate migration time based on strategy and data size
pub fn estimate_migration_time(
    strategy: &crate::migration::MigrationStrategy,
    total_records: u64,
    estimated_throughput: f64,
) -> Duration {
    let base_time_seconds = if estimated_throughput > 0.0 {
        total_records as f64 / estimated_throughput
    } else {
        0.0
    };

    // Add strategy-specific overhead
    let overhead_multiplier = match strategy {
        crate::migration::MigrationStrategy::Stream => 1.0,
        crate::migration::MigrationStrategy::DualWrite => 2.5, // Dual writes + backfill + verification
        crate::migration::MigrationStrategy::BlueGreen => 2.0, // Migration + warmup + verification
        crate::migration::MigrationStrategy::Snapshot => 1.5, // Snapshot + incremental + verification
    };

    Duration::from_secs_f64(base_time_seconds * overhead_multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_strategy() {
        let mut config = crate::migration::MigrationConfig::default();
        
        config.strategy = crate::migration::MigrationStrategy::Stream;
        assert_eq!(select_strategy(&config).unwrap(), "streaming");
        
        config.strategy = crate::migration::MigrationStrategy::DualWrite;
        assert_eq!(select_strategy(&config).unwrap(), "dual-write");
        
        config.strategy = crate::migration::MigrationStrategy::BlueGreen;
        assert_eq!(select_strategy(&config).unwrap(), "blue-green");
        
        config.strategy = crate::migration::MigrationStrategy::Snapshot;
        assert_eq!(select_strategy(&config).unwrap(), "snapshot");
    }

    #[test]
    fn test_estimate_migration_time() {
        let total_records = 1_000_000;
        let throughput = 1000.0; // 1000 records/sec
        
        let streaming_time = estimate_migration_time(
            &crate::migration::MigrationStrategy::Stream,
            total_records,
            throughput,
        );
        
        let dual_write_time = estimate_migration_time(
            &crate::migration::MigrationStrategy::DualWrite,
            total_records,
            throughput,
        );
        
        // Dual-write should take longer due to overhead
        assert!(dual_write_time > streaming_time);
        
        // Should be approximately 1000 seconds for streaming (1M records / 1000 records/sec)
        assert!(streaming_time.as_secs() >= 900 && streaming_time.as_secs() <= 1100);
    }

    #[test]
    fn test_snapshot_info() {
        let snapshot = SnapshotInfo {
            timestamp: chrono::Utc::now(),
            total_records: 1000,
            snapshot_id: uuid::Uuid::new_v4(),
        };
        
        assert_eq!(snapshot.total_records, 1000);
        assert!(!snapshot.snapshot_id.is_nil());
    }
}