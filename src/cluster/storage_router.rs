//! Storage Router for Distributed Operations
//!
//! Routes vector operations to the appropriate storage backend:
//! - Local operations: Direct CollectionManager access
//! - Remote operations: gRPC forwarding with scatter-gather
//!
//! Architecture:
//! ```text
//! Client Request
//!      |
//!      v
//! [StorageRouter] ----(local shard)----> [CollectionManager]
//!      |
//!      +----(remote shard)----> [ClusterClient] ----> [Remote Node]
//!      |
//!      +----(broadcast/all shards)----> [ScatterGather]
//! ```

#![cfg(feature = "grpc")]

use crate::collection::CollectionManager;
use crate::{
    SearchRequest as LocalSearchRequest, UpsertRequest, Vector, VectorId,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinSet;

use super::{
    client::ClusterClient,
    generated::SearchRequest,
    ClusterManager,
};

/// Timeout for remote operations (scatter-gather)
const REMOTE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum concurrent scatter requests
const MAX_CONCURRENT_SCATTER: usize = 100;

/// Storage router for distributed vector operations
pub struct StorageRouter {
    /// Local collection manager for direct storage access
    collections: Arc<RwLock<CollectionManager>>,
    /// Cluster manager for topology and routing decisions
    cluster: Arc<RwLock<ClusterManager>>,
    /// Cluster client for remote node communication
    client: Arc<ClusterClient>,
    /// Local node ID
    node_id: String,
}

impl StorageRouter {
    /// Create new storage router
    pub fn new(
        collections: Arc<RwLock<CollectionManager>>,
        cluster: Arc<RwLock<ClusterManager>>,
        client: Arc<ClusterClient>,
        node_id: String,
    ) -> Self {
        Self {
            collections,
            cluster,
            client,
            node_id,
        }
    }

    /// Search vectors in a collection
    ///
    /// # Architecture
    /// 1. Determine if query targets local or remote shard
    /// 2. If local: Execute search directly on CollectionManager
    /// 3. If remote: Forward via gRPC to responsible node
    /// 4. If broadcast: Use scatter-gather across all shards
    pub async fn search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
        broadcast: bool,
    ) -> crate::Result<Vec<ScoredResult>> {
        if broadcast {
            // Scatter-gather across all nodes
            self.scatter_search(collection, vector, top_k).await
        } else {
            // Single shard search
            self.single_shard_search(collection, vector, top_k).await
        }
    }

    /// Search a single shard (local or remote)
    async fn single_shard_search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<ScoredResult>> {
        // Determine which node owns this vector
        let target_node = {
            let cluster = self.cluster.read().await;
            // Use consistent hashing to find the node
            cluster.get_node_for_vector(rand::random::<u64>())
        };

        match target_node {
            Some(node_id) if node_id == self.node_id => {
                // Local execution
                self.local_search(collection, vector, top_k).await
            }
            Some(node_id) => {
                // Remote execution
                self.remote_search(&node_id, collection, vector, top_k).await
            }
            None => {
                // No nodes available, search locally as fallback
                self.local_search(collection, vector, top_k).await
            }
        }
    }

    /// Execute search on local storage
    async fn local_search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<ScoredResult>> {
        let collections = self.collections.read().await;
        
        let collection_ref = collections
            .get_collection(collection)
            .map_err(|_| crate::RTDBError::CollectionNotFound(collection.to_string()))?;

        let search_req = LocalSearchRequest {
            vector,
            limit: top_k as usize,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: false,
            filter: None,
            params: None,
        };

        let results = collection_ref.search(search_req)?;

        // Convert to ScoredResult
        let scored: Vec<ScoredResult> = results
            .into_iter()
            .map(|r| ScoredResult {
                id: r.id,
                score: r.score,
                node_id: self.node_id.clone(),
            })
            .collect();

        Ok(scored)
    }

    /// Forward search to remote node via gRPC
    async fn remote_search(
        &self,
        node_id: &str,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<ScoredResult>> {
        // Convert vector to bytes for protobuf
        let vector_bytes = vector_to_bytes(&vector);

        let _request = SearchRequest {
            collection: collection.to_string(),
            vector: vector_bytes,
            top_k,
            score_threshold: 0.0,
            filter: vec![],
            request_id: rand::random(),
        };

        // Send request with timeout
        match tokio::time::timeout(
            REMOTE_TIMEOUT,
            self.client.forward_search(node_id, collection, vector, top_k),
        )
        .await
        {
            Ok(Ok(results)) => {
                // Convert ProtoScoredVector to ScoredResult
                let scored: Vec<ScoredResult> = results
                    .into_iter()
                    .map(|r| ScoredResult {
                        id: r.id,
                        score: r.score,
                        node_id: node_id.to_string(),
                    })
                    .collect();
                Ok(scored)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(crate::RTDBError::Storage(format!(
                "Search timeout on node {}",
                node_id
            ))),
        }
    }

    /// Scatter-gather search across all shards
    ///
    /// # Pattern
    /// 1. **Scatter**: Send query to all nodes in parallel
    /// 2. **Gather**: Collect results with timeout
    /// 3. **Aggregate**: Merge, deduplicate, and rank results
    async fn scatter_search(
        &self,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<ScoredResult>> {
        // Get all active nodes
        let nodes: Vec<String> = {
            let cluster = self.cluster.read().await;
            cluster.active_nodes().into_iter().map(|n| n.id).collect()
        };

        if nodes.is_empty() {
            return self.local_search(collection, vector, top_k).await;
        }

        // Scatter: Send requests to all nodes in parallel
        let mut join_set = JoinSet::new();
        let vector_for_loop = vector.clone();

        for node_id in nodes {
            let client = Arc::clone(&self.client);
            let collection = collection.to_string();
            let vector_clone = vector_for_loop.clone();

            join_set.spawn(async move {
                // Use batch search for single vector (more efficient)
                match tokio::time::timeout(
                    REMOTE_TIMEOUT,
                    client.forward_batch_search(&node_id, &collection, vec![vector_clone], top_k),
                )
                .await
                {
                    Ok(Ok(results)) => (node_id, Ok(results.into_iter().flatten().collect::<Vec<_>>())),
                    Ok(Err(e)) => (node_id, Err(e)),
                    Err(_) => (node_id, Err(crate::RTDBError::Storage("Timeout".to_string()))),
                }
            });
        }

        // Gather: Collect results with partial failure handling
        let mut all_results: Vec<ScoredResult> = Vec::new();
        let mut failed_nodes = 0;

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((node_id, Ok(results))) => {
                    for r in results {
                        all_results.push(ScoredResult {
                            id: r.id,
                            score: r.score,
                            node_id: node_id.clone(),
                        });
                    }
                }
                Ok((node_id, Err(e))) => {
                    tracing::warn!("Search failed on node {}: {}", node_id, e);
                    failed_nodes += 1;
                }
                Err(e) => {
                    tracing::warn!("Search task failed: {}", e);
                    failed_nodes += 1;
                }
            }
        }

        // Check if we have any results or all nodes failed
        if all_results.is_empty() && failed_nodes > 0 {
            return Err(crate::RTDBError::Storage(
                "All scatter-gather searches failed".to_string(),
            ));
        }

        // Aggregate: Sort by score and take top_k
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        all_results.truncate(top_k as usize);

        Ok(all_results)
    }

    /// Insert vector into collection
    pub async fn insert(
        &self,
        collection: &str,
        id: VectorId,
        vector: Vector,
        payload: Option<crate::Payload>,
    ) -> crate::Result<()> {
        // Determine target node
        let target_node = {
            let cluster = self.cluster.read().await;
            cluster.get_node_for_vector(id)
        };

        match target_node {
            Some(node_id) if node_id == self.node_id => {
                // Local insert
                self.local_insert(collection, id, vector, payload).await
            }
            Some(node_id) => {
                // Forward to remote node
                self.remote_insert(&node_id, collection, id, vector).await
            }
            None => {
                // No cluster, insert locally
                self.local_insert(collection, id, vector, payload).await
            }
        }
    }

    /// Insert vector locally
    async fn local_insert(
        &self,
        collection: &str,
        id: VectorId,
        vector: Vector,
        _payload: Option<crate::Payload>,
    ) -> crate::Result<()> {
        let collections = self.collections.read().await;
        
        let collection_ref = collections
            .get_collection(collection)
            .map_err(|_| crate::RTDBError::CollectionNotFound(collection.to_string()))?;

        // Create upsert request for single vector
        let upsert_req = UpsertRequest {
            vectors: vec![(id, vector)],
        };

        collection_ref.upsert(upsert_req)?;
        Ok(())
    }

    /// Forward insert to remote node
    async fn remote_insert(
        &self,
        node_id: &str,
        collection: &str,
        id: VectorId,
        vector: Vector,
    ) -> crate::Result<()> {
        match tokio::time::timeout(
            REMOTE_TIMEOUT,
            self.client
                .forward_insert(node_id, collection, id, vector.data),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(crate::RTDBError::Storage(format!(
                "Insert timeout on node {}",
                node_id
            ))),
        }
    }

    /// Batch insert vectors (scatter to appropriate shards)
    pub async fn batch_insert(
        &self,
        collection: &str,
        vectors: Vec<(VectorId, Vector)>,
    ) -> crate::Result<BatchInsertResult> {
        // Group vectors by target node
        let mut node_groups: HashMap<String, Vec<(VectorId, Vec<f32>)>> = HashMap::new();

        for (id, vector) in vectors {
            let target_node = {
                let cluster = self.cluster.read().await;
                cluster.get_node_for_vector(id)
            };

            let node_id = target_node.unwrap_or_else(|| self.node_id.clone());
            node_groups
                .entry(node_id)
                .or_default()
                .push((id, vector.data));
        }

        // Send batches in parallel
        let mut join_set = JoinSet::new();
        
        for (node_id, group) in node_groups {
            let group_len = group.len() as u32;
            
            if node_id == self.node_id {
                // Local batch insert
                let collections = Arc::clone(&self.collections);
                let collection = collection.to_string();
                
                join_set.spawn(async move {
                    let cols = collections.read().await;
                    match cols.get_collection(&collection) {
                        Ok(col) => {
                            let vectors: Vec<(VectorId, Vector)> = group
                                .into_iter()
                                .map(|(id, data)| (id, Vector::new(data)))
                                .collect();
                            
                            let upsert_req = UpsertRequest { vectors };
                            match col.upsert(upsert_req) {
                                Ok(_) => (node_id, Ok(group_len)),
                                Err(e) => (node_id, Err(e)),
                            }
                        }
                        Err(e) => (node_id, Err(e)),
                    }
                });
            } else {
                // Remote batch insert
                let client = Arc::clone(&self.client);
                let collection = collection.to_string();
                
                join_set.spawn(async move {
                    match tokio::time::timeout(
                        REMOTE_TIMEOUT,
                        client.forward_batch_insert(&node_id, &collection, group),
                    )
                    .await
                    {
                        Ok(Ok(count)) => (node_id, Ok(count)),
                        Ok(Err(e)) => (node_id, Err(e)),
                        Err(_) => (node_id, Err(crate::RTDBError::Storage("Timeout".to_string()))),
                    }
                });
            }
        }

        // Collect results
        let mut total_inserted = 0u32;
        let mut failed_nodes = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((_node_id, Ok(count))) => {
                    total_inserted += count;
                }
                Ok((node_id, Err(e))) => {
                    tracing::warn!("Batch insert failed on node {}: {}", node_id, e);
                    failed_nodes.push(node_id);
                }
                Err(e) => {
                    tracing::warn!("Batch insert task failed: {}", e);
                }
            }
        }

        Ok(BatchInsertResult {
            inserted_count: total_inserted,
            failed_nodes,
        })
    }

    /// Replicate vector to follower nodes
    pub async fn replicate(
        &self,
        collection: &str,
        id: VectorId,
        vector: Vector,
        replica_nodes: &[String],
    ) -> crate::Result<()> {
        let mut join_set = JoinSet::new();

        for node_id in replica_nodes {
            if node_id == &self.node_id {
                // Local replication (already inserted)
                continue;
            }

            let client = Arc::clone(&self.client);
            let collection = collection.to_string();
            let node_id = node_id.clone();
            let vector_data = vector.data.clone();

            join_set.spawn(async move {
                match tokio::time::timeout(
                    REMOTE_TIMEOUT,
                    client.replicate(&node_id, &collection, id, vector_data),
                )
                .await
                {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(crate::RTDBError::Storage("Replication timeout".to_string())),
                }
            });
        }

        // Wait for all replications (best effort)
        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                tracing::warn!("Replication task failed: {}", e);
            }
        }

        Ok(())
    }
}

/// Scored search result with node attribution
#[derive(Debug, Clone)]
pub struct ScoredResult {
    /// Vector ID
    pub id: VectorId,
    /// Similarity score
    pub score: f32,
    /// Node that produced this result
    pub node_id: String,
}

/// Batch insert result
#[derive(Debug, Clone)]
pub struct BatchInsertResult {
    /// Number of vectors successfully inserted
    pub inserted_count: u32,
    /// Nodes that failed
    pub failed_nodes: Vec<String>,
}

/// Convert f32 vector to bytes for protobuf
fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|&f| f.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_bytes_conversion() {
        let original = vec![1.0f32, 2.5, 3.14];
        let bytes = vector_to_bytes(&original);
        
        // Verify size: 3 floats * 4 bytes each
        assert_eq!(bytes.len(), 12);
        
        // Verify roundtrip
        let recovered: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        
        assert_eq!(original, recovered);
    }
}
