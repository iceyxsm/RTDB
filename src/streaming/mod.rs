//! Real-time Streaming Vector Updates with CDC (Change Data Capture)
//!
//! This module provides real-time change data capture for vector collections,
//! enabling streaming updates to external consumers for:
//! - Real-time search index updates
//! - Replication to other systems
//! - Event-driven architectures
//! - Change notifications

use crate::{RTDBError, Vector, VectorId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

/// Type of change operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// Vector inserted
    Insert,
    /// Vector updated
    Update,
    /// Vector deleted
    Delete,
    /// Batch operation
    Batch,
}

/// Change event for a vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// Unique event ID
    pub event_id: String,
    /// Collection name
    pub collection: String,
    /// Vector ID
    pub vector_id: VectorId,
    /// Type of change
    pub change_type: ChangeType,
    /// Vector data (None for deletes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vector>,
    /// Previous vector data (for updates, None for inserts)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_vector: Option<Vector>,
    /// Event timestamp (microseconds since epoch)
    pub timestamp_us: u64,
    /// Transaction ID for grouping related changes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    /// Sequence number within transaction
    pub sequence_number: u64,
}

/// Configuration for CDC streaming
#[derive(Debug, Clone)]
pub struct CdcConfig {
    /// Maximum number of events to buffer per collection
    pub buffer_size: usize,
    /// Enable persistence of events to WAL
    pub persistent: bool,
    /// Retention time for events (seconds)
    pub retention_secs: u64,
    /// Maximum events per second (backpressure)
    pub max_events_per_sec: u32,
}

impl Default for CdcConfig {
    fn default() -> Self {
        Self {
            buffer_size: 10000,
            persistent: true,
            retention_secs: 3600, // 1 hour
            max_events_per_sec: 100000,
        }
    }
}

/// Subscription handle for CDC events
pub struct CdcSubscription {
    /// Subscription ID
    pub id: String,
    /// Collection being watched
    pub collection: String,
    /// Event receiver
    receiver: broadcast::Receiver<ChangeEvent>,
}

impl CdcSubscription {
    /// Receive next change event
    pub async fn recv(&mut self) -> Result<ChangeEvent, RTDBError> {
        self.receiver
            .recv()
            .await
            .map_err(|e| RTDBError::Io(format!("CDC stream closed: {}", e)))
    }

    /// Try to receive without blocking
    pub fn try_recv(&mut self) -> Result<Option<ChangeEvent>, RTDBError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(RTDBError::Io(format!("CDC receive error: {}", e))),
        }
    }

    /// Convert to a stream
    pub fn into_stream(self) -> BroadcastStream<ChangeEvent> {
        BroadcastStream::new(self.receiver)
    }
}

/// CDC Engine for managing change data capture
pub struct CdcEngine {
    /// Configuration
    config: CdcConfig,
    /// Broadcast channels per collection
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<ChangeEvent>>>>,
    /// Event sequence counter per collection
    sequence_counters: Arc<RwLock<HashMap<String, u64>>>,
}

impl CdcEngine {
    /// Create a new CDC engine
    pub fn new(config: CdcConfig) -> Self {
        Self {
            config,
            channels: Arc::new(RwLock::new(HashMap::new())),
            sequence_counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to changes for a collection
    pub async fn subscribe(&self, collection: &str) -> Result<CdcSubscription, RTDBError> {
        let mut channels = self.channels.write().await;

        // Get or create channel for this collection
        let sender = channels
            .entry(collection.to_string())
            .or_insert_with(|| broadcast::channel(self.config.buffer_size).0);

        let receiver = sender.subscribe();

        Ok(CdcSubscription {
            id: Uuid::new_v4().to_string(),
            collection: collection.to_string(),
            receiver,
        })
    }

    /// Publish a change event
    pub async fn publish(&self, event: ChangeEvent) -> Result<(), RTDBError> {
        let channels = self.channels.read().await;

        if let Some(sender) = channels.get(&event.collection) {
            // Non-blocking send - old events are dropped if buffer full
            let _ = sender.send(event);
        }

        Ok(())
    }

    /// Emit a vector insert event
    pub async fn emit_insert(
        &self,
        collection: &str,
        vector_id: VectorId,
        vector: Vector,
        transaction_id: Option<String>,
    ) -> Result<(), RTDBError> {
        let sequence = self.next_sequence(collection).await;

        let event = ChangeEvent {
            event_id: Uuid::new_v4().to_string(),
            collection: collection.to_string(),
            vector_id,
            change_type: ChangeType::Insert,
            vector: Some(vector),
            previous_vector: None,
            timestamp_us: Self::now_micros(),
            transaction_id,
            sequence_number: sequence,
        };

        self.publish(event).await
    }

    /// Emit a vector update event
    pub async fn emit_update(
        &self,
        collection: &str,
        vector_id: VectorId,
        new_vector: Vector,
        old_vector: Option<Vector>,
        transaction_id: Option<String>,
    ) -> Result<(), RTDBError> {
        let sequence = self.next_sequence(collection).await;

        let event = ChangeEvent {
            event_id: Uuid::new_v4().to_string(),
            collection: collection.to_string(),
            vector_id,
            change_type: ChangeType::Update,
            vector: Some(new_vector),
            previous_vector: old_vector,
            timestamp_us: Self::now_micros(),
            transaction_id,
            sequence_number: sequence,
        };

        self.publish(event).await
    }

    /// Emit a vector delete event
    pub async fn emit_delete(
        &self,
        collection: &str,
        vector_id: VectorId,
        old_vector: Option<Vector>,
        transaction_id: Option<String>,
    ) -> Result<(), RTDBError> {
        let sequence = self.next_sequence(collection).await;

        let event = ChangeEvent {
            event_id: Uuid::new_v4().to_string(),
            collection: collection.to_string(),
            vector_id,
            change_type: ChangeType::Delete,
            vector: None,
            previous_vector: old_vector,
            timestamp_us: Self::now_micros(),
            transaction_id,
            sequence_number: sequence,
        };

        self.publish(event).await
    }

    /// Emit a batch change event
    pub async fn emit_batch(
        &self,
        collection: &str,
        changes: Vec<(ChangeType, VectorId, Option<Vector>)>,
        transaction_id: Option<String>,
    ) -> Result<(), RTDBError> {
        let txn_id = transaction_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        for (idx, (change_type, vector_id, vector)) in changes.into_iter().enumerate() {
            let event = ChangeEvent {
                event_id: Uuid::new_v4().to_string(),
                collection: collection.to_string(),
                vector_id,
                change_type,
                vector: vector.clone(),
                previous_vector: None, // Simplified for batch
                timestamp_us: Self::now_micros(),
                transaction_id: Some(txn_id.clone()),
                sequence_number: idx as u64,
            };

            self.publish(event).await?;
        }

        Ok(())
    }

    /// Get next sequence number for a collection
    async fn next_sequence(&self, collection: &str) -> u64 {
        let mut counters = self.sequence_counters.write().await;
        let counter = counters.entry(collection.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Get current timestamp in microseconds
    fn now_micros() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    /// Get subscriber count for a collection
    pub async fn subscriber_count(&self, collection: &str) -> usize {
        let channels = self.channels.read().await;
        channels
            .get(collection)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }

    /// Unsubscribe all listeners for a collection
    pub async fn close_collection(&self, collection: &str) {
        let mut channels = self.channels.write().await;
        channels.remove(collection);
    }
}

/// CDC-aware collection wrapper
pub struct CdcCollection {
    /// Collection name
    name: String,
    /// CDC engine reference
    cdc_engine: Arc<CdcEngine>,
}

impl CdcCollection {
    /// Create a new CDC-aware collection wrapper
    pub fn new(name: String, cdc_engine: Arc<CdcEngine>) -> Self {
        Self { name, cdc_engine }
    }

    /// Emit insert event
    pub async fn on_insert(&self, vector_id: VectorId, vector: Vector) -> Result<(), RTDBError> {
        self.cdc_engine
            .emit_insert(&self.name, vector_id, vector, None)
            .await
    }

    /// Emit update event
    pub async fn on_update(
        &self,
        vector_id: VectorId,
        new_vector: Vector,
        old_vector: Option<Vector>,
    ) -> Result<(), RTDBError> {
        self.cdc_engine
            .emit_update(&self.name, vector_id, new_vector, old_vector, None)
            .await
    }

    /// Emit delete event
    pub async fn on_delete(&self, vector_id: VectorId, old_vector: Option<Vector>) -> Result<(), RTDBError> {
        self.cdc_engine
            .emit_delete(&self.name, vector_id, old_vector, None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cdc_basic_flow() {
        let cdc = CdcEngine::new(CdcConfig::default());

        // Subscribe to changes
        let mut sub = cdc.subscribe("test_collection").await.unwrap();

        // Emit an insert
        let vector = Vector::new(vec![1.0, 2.0, 3.0]);
        cdc.emit_insert("test_collection", 1, vector.clone(), None)
            .await
            .unwrap();

        // Receive the event
        let event = sub.recv().await.unwrap();
        assert_eq!(event.collection, "test_collection");
        assert_eq!(event.vector_id, 1);
        assert_eq!(event.change_type, ChangeType::Insert);
        assert!(event.vector.is_some());
    }

    #[tokio::test]
    async fn test_cdc_multiple_subscribers() {
        let cdc = CdcEngine::new(CdcConfig::default());

        // Multiple subscribers
        let mut sub1 = cdc.subscribe("col").await.unwrap();
        let mut sub2 = cdc.subscribe("col").await.unwrap();

        // Emit event
        cdc.emit_insert("col", 1, Vector::new(vec![1.0]), None)
            .await
            .unwrap();

        // Both receive it
        let e1 = sub1.recv().await.unwrap();
        let e2 = sub2.recv().await.unwrap();
        assert_eq!(e1.event_id, e2.event_id);
    }

    #[tokio::test]
    async fn test_cdc_transaction_events() {
        let cdc = CdcEngine::new(CdcConfig::default());
        let mut sub = cdc.subscribe("col").await.unwrap();

        let txn_id = "txn-123".to_string();

        // Emit batch as transaction
        let changes = vec![
            (ChangeType::Insert, 1, Some(Vector::new(vec![1.0]))),
            (ChangeType::Insert, 2, Some(Vector::new(vec![2.0]))),
            (ChangeType::Insert, 3, Some(Vector::new(vec![3.0]))),
        ];

        cdc.emit_batch("col", changes, Some(txn_id.clone()))
            .await
            .unwrap();

        // All events have same transaction ID
        for i in 0..3 {
            let event = sub.recv().await.unwrap();
            assert_eq!(event.transaction_id, Some(txn_id.clone()));
            assert_eq!(event.sequence_number, i as u64);
        }
    }
}
