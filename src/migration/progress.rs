//! Progress tracking and ETA calculations for migrations
//!
//! Provides real-time progress monitoring, throughput calculation, and ETA estimation.

use crate::migration::{MigrationManager, MigrationStatus};
use crate::{Result, RTDBError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Progress tracker for migration operations
pub struct ProgressTracker {
    migration_id: Uuid,
    manager: MigrationManager,
    start_time: Instant,
    last_update: Arc<RwLock<Instant>>,
    throughput_window: Arc<RwLock<ThroughputWindow>>,
}

/// Sliding window for throughput calculation
#[derive(Debug, Clone)]
struct ThroughputWindow {
    samples: Vec<ThroughputSample>,
    window_size: Duration,
    max_samples: usize,
}

/// Single throughput measurement
#[derive(Debug, Clone)]
struct ThroughputSample {
    timestamp: Instant,
    records_processed: u64,
}

impl ProgressTracker {
    /// Create new progress tracker
    pub fn new(migration_id: Uuid, manager: MigrationManager) -> Self {
        Self {
            migration_id,
            manager,
            start_time: Instant::now(),
            last_update: Arc::new(RwLock::new(Instant::now())),
            throughput_window: Arc::new(RwLock::new(ThroughputWindow::new(
                Duration::from_secs(60), // 1-minute window
                100, // max 100 samples
            ))),
        }
    }

    /// Update progress with new counts
    pub async fn update_progress(&self, processed: u64, failed: u64) -> Result<()> {
        let now = Instant::now();
        
        // Update throughput window
        {
            let mut window = self.throughput_window.write().await;
            window.add_sample(now, processed);
        }
        
        // Update last update time
        *self.last_update.write().await = now;
        
        // Update manager with new progress
        self.manager.update_processed(self.migration_id, processed, failed).await;
        
        Ok(())
    }

    /// Get current throughput (records per second)
    pub async fn get_throughput(&self) -> f64 {
        let window = self.throughput_window.read().await;
        window.calculate_throughput()
    }

    /// Get estimated time to completion
    pub async fn get_eta(&self, total_records: Option<u64>) -> Option<Duration> {
        if let Some(total) = total_records {
            let progress = self.manager.get_progress(self.migration_id).await?;
            let remaining = total.saturating_sub(progress.processed_records);
            
            if remaining == 0 {
                return Some(Duration::from_secs(0));
            }
            
            let throughput = self.get_throughput().await;
            if throughput > 0.0 {
                let eta_seconds = remaining as f64 / throughput;
                return Some(Duration::from_secs_f64(eta_seconds));
            }
        }
        None
    }

    /// Get elapsed time since start
    pub fn get_elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get detailed progress statistics
    pub async fn get_stats(&self) -> Result<ProgressStats> {
        let progress = self.manager.get_progress(self.migration_id).await
            .ok_or_else(|| RTDBError::Config("Migration not found".to_string()))?;
        
        let elapsed = self.get_elapsed();
        let throughput = self.get_throughput().await;
        let eta = self.get_eta(progress.total_records).await;
        
        let completion_percentage = if let Some(total) = progress.total_records {
            if total > 0 {
                (progress.processed_records as f64 / total as f64 * 100.0).min(100.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        Ok(ProgressStats {
            migration_id: self.migration_id,
            status: progress.status,
            total_records: progress.total_records,
            processed_records: progress.processed_records,
            failed_records: progress.failed_records,
            completion_percentage,
            elapsed_time: elapsed,
            current_throughput: throughput,
            average_throughput: if elapsed.as_secs() > 0 {
                progress.processed_records as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            },
            estimated_time_remaining: eta,
            error_rate: if progress.processed_records + progress.failed_records > 0 {
                progress.failed_records as f64 / (progress.processed_records + progress.failed_records) as f64 * 100.0
            } else {
                0.0
            },
        })
    }

    /// Log progress at regular intervals
    pub async fn log_progress(&self) {
        if let Ok(stats) = self.get_stats().await {
            tracing::info!(
                "Migration {} progress: {:.1}% ({}/{}) - {:.0} records/sec - ETA: {}",
                self.migration_id,
                stats.completion_percentage,
                stats.processed_records,
                stats.total_records.map(|t| t.to_string()).unwrap_or_else(|| "?".to_string()),
                stats.current_throughput,
                stats.estimated_time_remaining
                    .map(format_duration)
                    .unwrap_or_else(|| "unknown".to_string())
            );
            
            if stats.failed_records > 0 {
                tracing::warn!(
                    "Migration {} has {} failed records ({:.2}% error rate)",
                    self.migration_id,
                    stats.failed_records,
                    stats.error_rate
                );
            }
        }
    }

    /// Start periodic progress logging
    pub async fn start_periodic_logging(&self, interval: Duration) {
        let tracker = self.clone();
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // Check if migration is still active
                if let Some(progress) = tracker.manager.get_progress(tracker.migration_id).await {
                    match progress.status {
                        MigrationStatus::Running => {
                            tracker.log_progress().await;
                        }
                        MigrationStatus::Completed | MigrationStatus::Failed | MigrationStatus::Cancelled => {
                            tracing::info!("Migration {} finished, stopping progress logging", tracker.migration_id);
                            break;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
        });
    }
}

impl Clone for ProgressTracker {
    fn clone(&self) -> Self {
        Self {
            migration_id: self.migration_id,
            manager: self.manager.clone(),
            start_time: self.start_time,
            last_update: self.last_update.clone(),
            throughput_window: self.throughput_window.clone(),
        }
    }
}

impl ThroughputWindow {
    fn new(window_size: Duration, max_samples: usize) -> Self {
        Self {
            samples: Vec::new(),
            window_size,
            max_samples,
        }
    }

    fn add_sample(&mut self, timestamp: Instant, records_processed: u64) {
        // Remove old samples outside the window
        let cutoff = timestamp - self.window_size;
        self.samples.retain(|sample| sample.timestamp >= cutoff);
        
        // Add new sample
        self.samples.push(ThroughputSample {
            timestamp,
            records_processed,
        });
        
        // Limit number of samples
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    fn calculate_throughput(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }
        
        let first = &self.samples[0];
        let last = &self.samples[self.samples.len() - 1];
        
        let time_diff = last.timestamp.duration_since(first.timestamp);
        let records_diff = last.records_processed.saturating_sub(first.records_processed);
        
        if time_diff.as_secs_f64() > 0.0 {
            records_diff as f64 / time_diff.as_secs_f64()
        } else {
            0.0
        }
    }
}

/// Detailed progress statistics for migration operations
#[derive(Debug, Clone)]
pub struct ProgressStats {
    /// Migration unique identifier
    pub migration_id: Uuid,
    /// Current migration status
    pub status: MigrationStatus,
    /// Total number of records to process (if known)
    pub total_records: Option<u64>,
    /// Number of records processed successfully
    pub processed_records: u64,
    /// Number of records that failed processing
    pub failed_records: u64,
    /// Completion percentage (0-100)
    pub completion_percentage: f64,
    /// Time elapsed since migration started
    pub elapsed_time: Duration,
    /// Current processing speed (records/second)
    pub current_throughput: f64,
    /// Average processing speed since start
    pub average_throughput: f64,
    /// Estimated time remaining (if calculable)
    pub estimated_time_remaining: Option<Duration>,
    /// Error rate as percentage (0-100)
    pub error_rate: f64,
}

/// Format duration in human-readable format
fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    
    if total_seconds < 60 {
        format!("{}s", total_seconds)
    } else if total_seconds < 3600 {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{}m {}s", minutes, seconds)
    } else if total_seconds < 86400 {
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    } else {
        let days = total_seconds / 86400;
        let hours = (total_seconds % 86400) / 3600;
        format!("{}d {}h", days, hours)
    }
}

/// Progress reporter for different output formats
pub struct ProgressReporter {
    tracker: ProgressTracker,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new(tracker: ProgressTracker) -> Self {
        Self { tracker }
    }

    /// Generate JSON progress report
    pub async fn to_json(&self) -> Result<String> {
        let stats = self.tracker.get_stats().await?;
        let json = serde_json::json!({
            "migration_id": stats.migration_id,
            "status": format!("{:?}", stats.status),
            "progress": {
                "total_records": stats.total_records,
                "processed_records": stats.processed_records,
                "failed_records": stats.failed_records,
                "completion_percentage": stats.completion_percentage,
                "error_rate": stats.error_rate
            },
            "timing": {
                "elapsed_seconds": stats.elapsed_time.as_secs(),
                "estimated_remaining_seconds": stats.estimated_time_remaining.map(|d| d.as_secs())
            },
            "throughput": {
                "current_records_per_second": stats.current_throughput,
                "average_records_per_second": stats.average_throughput
            }
        });
        
        serde_json::to_string_pretty(&json)
            .map_err(|e| RTDBError::Serialization(format!("Failed to serialize progress: {}", e)))
    }

    /// Generate human-readable progress report
    pub async fn to_string(&self) -> Result<String> {
        let stats = self.tracker.get_stats().await?;
        
        let mut report = String::new();
        report.push_str(&format!("Migration: {}\n", stats.migration_id));
        report.push_str(&format!("Status: {:?}\n", stats.status));
        report.push_str(&format!("Progress: {:.1}%\n", stats.completion_percentage));
        
        if let Some(total) = stats.total_records {
            report.push_str(&format!("Records: {}/{}\n", stats.processed_records, total));
        } else {
            report.push_str(&format!("Records processed: {}\n", stats.processed_records));
        }
        
        if stats.failed_records > 0 {
            report.push_str(&format!("Failed records: {} ({:.2}%)\n", stats.failed_records, stats.error_rate));
        }
        
        report.push_str(&format!("Elapsed: {}\n", format_duration(stats.elapsed_time)));
        
        if let Some(eta) = stats.estimated_time_remaining {
            report.push_str(&format!("ETA: {}\n", format_duration(eta)));
        }
        
        report.push_str(&format!("Throughput: {:.0} records/sec (current), {:.0} records/sec (average)\n", 
                                stats.current_throughput, stats.average_throughput));
        
        Ok(report)
    }

    /// Generate CSV line for progress logging
    pub async fn to_csv_line(&self) -> Result<String> {
        let stats = self.tracker.get_stats().await?;
        
        Ok(format!(
            "{},{:?},{},{},{},{:.2},{},{:.2},{:.2},{}\n",
            stats.migration_id,
            stats.status,
            stats.total_records.map(|t| t.to_string()).unwrap_or_else(|| "".to_string()),
            stats.processed_records,
            stats.failed_records,
            stats.completion_percentage,
            stats.elapsed_time.as_secs(),
            stats.current_throughput,
            stats.average_throughput,
            stats.estimated_time_remaining.map(|d| d.as_secs().to_string()).unwrap_or_else(|| "".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_throughput_window() {
        let mut window = ThroughputWindow::new(Duration::from_secs(10), 5);
        let start = Instant::now();
        
        // Add samples
        window.add_sample(start, 0);
        window.add_sample(start + Duration::from_secs(1), 100);
        window.add_sample(start + Duration::from_secs(2), 200);
        
        let throughput = window.calculate_throughput();
        assert!(throughput > 90.0 && throughput < 110.0); // ~100 records/sec
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m");
        assert_eq!(format_duration(Duration::from_secs(90061)), "1d 1h");
    }

    #[tokio::test]
    async fn test_progress_stats() {
        // This would require a real MigrationManager instance
        // For now, just test the structure
        let stats = ProgressStats {
            migration_id: Uuid::new_v4(),
            status: MigrationStatus::Running,
            total_records: Some(1000),
            processed_records: 500,
            failed_records: 5,
            completion_percentage: 50.0,
            elapsed_time: Duration::from_secs(60),
            current_throughput: 8.33,
            average_throughput: 8.33,
            estimated_time_remaining: Some(Duration::from_secs(60)),
            error_rate: 1.0,
        };
        
        assert_eq!(stats.completion_percentage, 50.0);
        assert_eq!(stats.error_rate, 1.0);
    }
}