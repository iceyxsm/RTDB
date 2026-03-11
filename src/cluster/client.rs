//! Cluster gRPC Client
//!
//! Client for communicating with other nodes in the cluster.

#![cfg(grpc)]

use super::{
    ClusterConfig, NodeInfo,
    proto::cluster_service_client::ClusterServiceClient,
    proto::{
        HeartbeatRequest, InsertRequest, JoinRequest, LeaveRequest, ReplicateRequest,
        SearchRequest, TopologyRequest,
    },
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::transport::Channel;

/// Cluster client manager
///
/// Manages connections to all other nodes in the cluster.
pub struct ClusterClient {
    /// Local node configuration
    config: ClusterConfig,
    /// Cached connections to other nodes
    connections: Arc<RwLock<HashMap<String, ClusterServiceClient<Channel>>>>,
}

impl ClusterClient {
    /// Create new cluster client
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Connect to a node
    pub async fn connect(&self, node: &NodeInfo) -> crate::Result<()> {
        let addr = format!("http://{}", node.address);
        
        let client = ClusterServiceClient::connect(addr)
            .await
            .map_err(|e| crate::RTDBError::Io(format!("Failed to connect to {}: {}", node.id, e)))?;

        self.connections.write().await.insert(node.id.clone(), client);
        
        tracing::info!("Connected to node {} at {}", node.id, node.address);
        Ok(())
    }

    /// Disconnect from a node
    pub async fn disconnect(&self, node_id: &str) {
        self.connections.write().await.remove(node_id);
        tracing::info!("Disconnected from node {}", node_id);
    }

    /// Join cluster by contacting seed nodes
    pub async fn join_cluster(&self, seeds: &[String]) -> crate::Result<NodeInfo> {
        let local_node = NodeInfo {
            id: self.config.node_id.clone(),
            address: self.config.bind_addr,
            status: super::NodeStatus::Joining,
            shards: vec![],
            capacity: 100_000_000, // 100M vectors
            load: 0,
            last_heartbeat: current_timestamp(),
        };

        // Try each seed node
        for seed_addr in seeds {
            match self.try_join_seed(seed_addr, &local_node).await {
                Ok(topology) => {
                    tracing::info!("Successfully joined cluster via {}", seed_addr);
                    // Connect to all nodes in topology
                    for node in &topology.nodes {
                        if node.id != self.config.node_id {
                            // Parse address and connect
                            if let Ok(addr) = node.address.parse() {
                                let node_info = NodeInfo {
                                    id: node.id.clone(),
                                    address: addr,
                                    status: super::NodeStatus::Active,
                                    shards: node.shards.clone(),
                                    capacity: node.capacity as usize,
                                    load: node.load as usize,
                                    last_heartbeat: current_timestamp(),
                                };
                                let _ = self.connect(&node_info).await;
                            }
                        }
                    }
                    return Ok(local_node);
                }
                Err(e) => {
                    tracing::warn!("Failed to join via {}: {}", seed_addr, e);
                }
            }
        }

        Err(crate::RTDBError::Storage(
            "Failed to join cluster via any seed node".to_string()
        ))
    }

    /// Try to join via a specific seed node
    async fn try_join_seed(
        &self,
        seed_addr: &str,
        local_node: &NodeInfo,
    ) -> crate::Result<super::proto::Topology> {
        let addr = format!("http://{}", seed_addr);
        
        let mut client = ClusterServiceClient::connect(addr)
            .await
            .map_err(|e| crate::RTDBError::Io(format!("Connection failed: {}", e)))?;

        let request = tonic::Request::new(JoinRequest {
            node_id: local_node.id.clone(),
            address: local_node.address.to_string(),
            capacity: local_node.capacity as u64,
        });

        let response = client.join_cluster(request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Join failed: {}", e)))?;

        let join_response = response.into_inner();
        
        if !join_response.success {
            return Err(crate::RTDBError::Storage(
                join_response.error.clone()
            ));
        }

        join_response.topology.ok_or_else(|| {
            crate::RTDBError::Storage("No topology in join response".to_string())
        })
    }

    /// Send heartbeat to a specific node
    pub async fn send_heartbeat(&self, node_id: &str) -> crate::Result<()> {
        let mut clients = self.connections.write().await;
        
        if let Some(client) = clients.get_mut(node_id) {
            let request = tonic::Request::new(HeartbeatRequest {
                node_id: self.config.node_id.clone(),
                timestamp: current_timestamp(),
                load: 0, // TODO: Get actual load
            });

            match client.heartbeat(request).await {
                Ok(response) => {
                    if response.into_inner().acknowledged {
                        Ok(())
                    } else {
                        Err(crate::RTDBError::Storage(
                            "Heartbeat not acknowledged".to_string()
                        ))
                    }
                }
                Err(e) => {
                    // Remove failed connection
                    clients.remove(node_id);
                    Err(crate::RTDBError::Io(format!("Heartbeat failed: {}", e)))
                }
            }
        } else {
            Err(crate::RTDBError::Storage(
                format!("No connection to node {}", node_id)
            ))
        }
    }

    /// Forward search request to another node
    pub async fn forward_search(
        &self,
        node_id: &str,
        collection: &str,
        vector: Vec<f32>,
        top_k: u32,
    ) -> crate::Result<Vec<super::proto::ScoredVector>> {
        let mut clients = self.connections.write().await;
        
        let client = clients.get_mut(node_id).ok_or_else(|| {
            crate::RTDBError::Storage(format!("No connection to node {}", node_id))
        })?;

        let request = tonic::Request::new(SearchRequest {
            collection: collection.to_string(),
            vector,
            top_k,
            score_threshold: 0.0,
        });

        let response = client.search(request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Search failed: {}", e)))?;

        Ok(response.into_inner().results)
    }

    /// Forward insert request to another node
    pub async fn forward_insert(
        &self,
        node_id: &str,
        collection: &str,
        id: u64,
        vector: Vec<f32>,
    ) -> crate::Result<()> {
        let mut clients = self.connections.write().await;
        
        let client = clients.get_mut(node_id).ok_or_else(|| {
            crate::RTDBError::Storage(format!("No connection to node {}", node_id))
        })?;

        let request = tonic::Request::new(InsertRequest {
            collection: collection.to_string(),
            id,
            vector,
            payload: vec![],
        });

        let response = client.insert(request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Insert failed: {}", e)))?;

        if response.into_inner().success {
            Ok(())
        } else {
            Err(crate::RTDBError::Storage("Insert failed".to_string()))
        }
    }

    /// Replicate data to a follower node
    pub async fn replicate(
        &self,
        node_id: &str,
        collection: &str,
        id: u64,
        vector: Vec<f32>,
    ) -> crate::Result<()> {
        let mut clients = self.connections.write().await;
        
        let client = clients.get_mut(node_id).ok_or_else(|| {
            crate::RTDBError::Storage(format!("No connection to node {}", node_id))
        })?;

        let request = tonic::Request::new(ReplicateRequest {
            collection: collection.to_string(),
            id,
            vector,
            payload: vec![],
            timestamp: current_timestamp(),
        });

        let response = client.replicate(request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Replication failed: {}", e)))?;

        if response.into_inner().success {
            Ok(())
        } else {
            Err(crate::RTDBError::Storage("Replication failed".to_string()))
        }
    }

    /// Get topology from another node
    pub async fn get_topology(&self, node_id: &str) -> crate::Result<super::proto::Topology> {
        let mut clients = self.connections.write().await;
        
        let client = clients.get_mut(node_id).ok_or_else(|| {
            crate::RTDBError::Storage(format!("No connection to node {}", node_id))
        })?;

        let request = tonic::Request::new(TopologyRequest {});

        let response = client.get_topology(request)
            .await
            .map_err(|e| crate::RTDBError::Storage(format!("Get topology failed: {}", e)))?;

        response.into_inner().topology.ok_or_else(|| {
            crate::RTDBError::Storage("No topology in response".to_string())
        })
    }

    /// Get all connected node IDs
    pub async fn connected_nodes(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// Check if connected to a node
    pub async fn is_connected(&self, node_id: &str) -> bool {
        self.connections.read().await.contains_key(node_id)
    }
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_client_creation() {
        let config = ClusterConfig::default();
        let client = ClusterClient::new(config);
        
        // Just verify it compiles and creates
        assert_eq!(client.config.node_id, ClusterConfig::default().node_id);
    }
}
