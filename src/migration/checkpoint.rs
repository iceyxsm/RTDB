//! Checkpoint management for resumable migrations
//!
//! Provides persistent checkpoint storage to enable resuming interrupted migrations.
//! Checkpoints contain migration state, progress information, and recovery data.

use crate::{Result, RTDBError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

/// Checkpoint data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Migration ID
    pub migration_id: Uuid,
    /// Checkpoint timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Current offset in source data
    pub offset: u64,
    /// Current batch ID
    pub batch_id: u64,
    /// Records processed so far
    pub processed_records: u64,
    /// Records failed so far
    pub failed_records: u64,
    /// Source-specific state
    pub source_state: serde_json::Value,
    /// Target-specific state
    pub target_state: serde_json::Value,
    /// Migration configuration hash (for validation)
    pub config_hash: String,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Checkpoint manager for persistent storage
#[derive(Clone)]
pub struct CheckpointManager {
    /// Directory for checkpoint files
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    /// Create new checkpoint manager
    pub fn new(checkpoint_dir: PathBuf) -> Result<Self> {
        Ok(Self { checkpoint_dir })
    }

    /// Initialize checkpoint directory
    pub async fn init(&self) -> Result<()> {
        if !self.checkpoint_dir.exists() {
            fs::create_dir_all(&self.checkpoint_dir).await
                .map_err(|e| RTDBError::Io(format!("Failed to create checkpoint directory: {}", e)))?;
        }
        Ok(())
    }

    /// Save checkpoint to disk
    pub async fn save_checkpoint(
        &self,
        migration_id: Uuid,
        checkpoint_data: serde_json::Value,
    ) -> Result<()> {
        self.init().await?;

        let checkpoint_path = self.get_checkpoint_path(migration_id);
        let checkpoint_json = serde_json::to_string_pretty(&checkpoint_data)
            .map_err(|e| RTDBError::Serialization(format!("Failed to serialize checkpoint: {}", e)))?;

        // Write to temporary file first, then rename for atomicity
        let temp_path = checkpoint_path.with_extension("tmp");
        fs::write(&temp_path, checkpoint_json).await
            .map_err(|e| RTDBError::Io(format!("Failed to write checkpoint: {}", e)))?;

        fs::rename(&temp_path, &checkpoint_path).await
            .map_err(|e| RTDBError::Io(format!("Failed to rename checkpoint: {}", e)))?;

        tracing::debug!("Saved checkpoint for migration {}", migration_id);
        Ok(())
    }

    /// Load checkpoint from disk
    pub async fn load_checkpoint(&self, migration_id: Uuid) -> Result<Option<serde_json::Value>> {
        let checkpoint_path = self.get_checkpoint_path(migration_id);
        
        if !checkpoint_path.exists() {
            return Ok(None);
        }

        let checkpoint_data = fs::read_to_string(&checkpoint_path).await
            .map_err(|e| RTDBError::Io(format!("Failed to read checkpoint: {}", e)))?;

        let checkpoint: serde_json::Value = serde_json::from_str(&checkpoint_data)
            .map_err(|e| RTDBError::Serialization(format!("Failed to deserialize checkpoint: {}", e)))?;

        tracing::debug!("Loaded checkpoint for migration {}", migration_id);
        Ok(Some(checkpoint))
    }

    /// Delete checkpoint
    pub async fn delete_checkpoint(&self, migration_id: Uuid) -> Result<()> {
        let checkpoint_path = self.get_checkpoint_path(migration_id);
        
        if checkpoint_path.exists() {
            fs::remove_file(&checkpoint_path).await
                .map_err(|e| RTDBError::Io(format!("Failed to delete checkpoint: {}", e)))?;
            tracing::debug!("Deleted checkpoint for migration {}", migration_id);
        }
        
        Ok(())
    }

    /// List all checkpoints
    pub async fn list_checkpoints(&self) -> Result<Vec<Uuid>> {
        if !self.checkpoint_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = fs::read_dir(&self.checkpoint_dir).await
            .map_err(|e| RTDBError::Io(format!("Failed to read checkpoint directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| RTDBError::Io(format!("Failed to read directory entry: {}", e)))? {
            
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(migration_id) = Uuid::parse_str(filename) {
                        checkpoints.push(migration_id);
                    }
                }
            }
        }

        Ok(checkpoints)
    }

    /// Create structured checkpoint
    pub async fn create_checkpoint(
        &self,
        migration_id: Uuid,
        offset: u64,
        batch_id: u64,
        processed_records: u64,
        failed_records: u64,
        source_state: serde_json::Value,
        target_state: serde_json::Value,
        config_hash: String,
    ) -> Result<Checkpoint> {
        let checkpoint = Checkpoint {
            migration_id,
            timestamp: chrono::Utc::now(),
            offset,
            batch_id,
            processed_records,
            failed_records,
            source_state,
            target_state,
            config_hash,
            metadata: HashMap::new(),
        };

        let checkpoint_json = serde_json::to_value(&checkpoint)
            .map_err(|e| RTDBError::Serialization(format!("Failed to serialize checkpoint: {}", e)))?;

        self.save_checkpoint(migration_id, checkpoint_json).await?;
        Ok(checkpoint)
    }

    /// Load structured checkpoint
    pub async fn load_structured_checkpoint(&self, migration_id: Uuid) -> Result<Option<Checkpoint>> {
        if let Some(checkpoint_data) = self.load_checkpoint(migration_id).await? {
            let checkpoint: Checkpoint = serde_json::from_value(checkpoint_data)
                .map_err(|e| RTDBError::Serialization(format!("Failed to deserialize checkpoint: {}", e)))?;
            Ok(Some(checkpoint))
        } else {
            Ok(None)
        }
    }

    /// Validate checkpoint against current configuration
    pub fn validate_checkpoint(&self, checkpoint: &Checkpoint, current_config_hash: &str) -> Result<()> {
        if checkpoint.config_hash != current_config_hash {
            return Err(RTDBError::Config(
                "Checkpoint configuration hash mismatch - migration config has changed".to_string()
            ));
        }
        Ok(())
    }

    /// Get checkpoint file path
    fn get_checkpoint_path(&self, migration_id: Uuid) -> PathBuf {
        self.checkpoint_dir.join(format!("{}.json", migration_id))
    }

    /// Clean up old checkpoints (older than specified days)
    pub async fn cleanup_old_checkpoints(&self, days: u32) -> Result<usize> {
        if !self.checkpoint_dir.exists() {
            return Ok(0);
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let mut cleaned = 0;
        let mut entries = fs::read_dir(&self.checkpoint_dir).await
            .map_err(|e| RTDBError::Io(format!("Failed to read checkpoint directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| RTDBError::Io(format!("Failed to read directory entry: {}", e)))? {
            
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        let modified_dt = chrono::DateTime::<chrono::Utc>::from(modified);
                        if modified_dt < cutoff {
                            if let Err(e) = fs::remove_file(&path).await {
                                tracing::warn!("Failed to remove old checkpoint {:?}: {}", path, e);
                            } else {
                                cleaned += 1;
                                tracing::debug!("Cleaned up old checkpoint: {:?}", path);
                            }
                        }
                    }
                }
            }
        }

        tracing::info!("Cleaned up {} old checkpoints", cleaned);
        Ok(cleaned)
    }

    /// Get checkpoint statistics
    pub async fn get_checkpoint_stats(&self) -> Result<CheckpointStats> {
        let checkpoints = self.list_checkpoints().await?;
        let mut total_size = 0u64;
        let mut oldest_timestamp = None;
        let mut newest_timestamp = None;

        for migration_id in &checkpoints {
            let checkpoint_path = self.get_checkpoint_path(*migration_id);
            if let Ok(metadata) = fs::metadata(&checkpoint_path).await {
                total_size += metadata.len();
                
                if let Ok(modified) = metadata.modified() {
                    let modified_dt = chrono::DateTime::<chrono::Utc>::from(modified);
                    
                    if oldest_timestamp.is_none() || Some(modified_dt) < oldest_timestamp {
                        oldest_timestamp = Some(modified_dt);
                    }
                    
                    if newest_timestamp.is_none() || Some(modified_dt) > newest_timestamp {
                        newest_timestamp = Some(modified_dt);
                    }
                }
            }
        }

        Ok(CheckpointStats {
            count: checkpoints.len(),
            total_size_bytes: total_size,
            oldest_timestamp,
            newest_timestamp,
        })
    }
}

/// Checkpoint statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointStats {
    /// Number of checkpoints
    pub count: usize,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Oldest checkpoint timestamp
    pub oldest_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Newest checkpoint timestamp
    pub newest_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checkpoint_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        let migration_id = Uuid::new_v4();
        let checkpoint_data = serde_json::json!({
            "offset": 1000,
            "batch_id": 10,
            "timestamp": chrono::Utc::now()
        });

        // Save checkpoint
        manager.save_checkpoint(migration_id, checkpoint_data.clone()).await.unwrap();

        // Load checkpoint
        let loaded = manager.load_checkpoint(migration_id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap()["offset"], 1000);
    }

    #[tokio::test]
    async fn test_structured_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        let migration_id = Uuid::new_v4();
        let checkpoint = manager.create_checkpoint(
            migration_id,
            1000,
            10,
            5000,
            5,
            serde_json::json!({"cursor": "abc123"}),
            serde_json::json!({"last_id": 999}),
            "config_hash_123".to_string(),
        ).await.unwrap();

        // Load structured checkpoint
        let loaded = manager.load_structured_checkpoint(migration_id).await.unwrap();
        assert!(loaded.is_some());
        
        let loaded_checkpoint = loaded.unwrap();
        assert_eq!(loaded_checkpoint.offset, 1000);
        assert_eq!(loaded_checkpoint.batch_id, 10);
        assert_eq!(loaded_checkpoint.processed_records, 5000);
        assert_eq!(loaded_checkpoint.failed_records, 5);
    }

    #[tokio::test]
    async fn test_checkpoint_validation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        let checkpoint = Checkpoint {
            migration_id: Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            offset: 0,
            batch_id: 0,
            processed_records: 0,
            failed_records: 0,
            source_state: serde_json::Value::Null,
            target_state: serde_json::Value::Null,
            config_hash: "hash123".to_string(),
            metadata: HashMap::new(),
        };

        // Valid hash
        assert!(manager.validate_checkpoint(&checkpoint, "hash123").is_ok());
        
        // Invalid hash
        assert!(manager.validate_checkpoint(&checkpoint, "hash456").is_err());
    }

    #[tokio::test]
    async fn test_list_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path().to_path_buf()).unwrap();
        
        // Initially empty
        let checkpoints = manager.list_checkpoints().await.unwrap();
        assert_eq!(checkpoints.len(), 0);

        // Create some checkpoints
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        
        manager.save_checkpoint(id1, serde_json::json!({"test": 1})).await.unwrap();
        manager.save_checkpoint(id2, serde_json::json!({"test": 2})).await.unwrap();

        // Should list both
        let checkpoints = manager.list_checkpoints().await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints.contains(&id1));
        assert!(checkpoints.contains(&id2));
    }
}