//! Cluster gRPC Server
//!
//! Handles incoming inter-node communication requests.

#![cfg(grpc)]

use super::{
    ClusterManager, NodeInfo, NodeStatus,
    proto::cluster_service_server::{ClusterService, ClusterServiceServer},
    proto::{
        HeartbeatRequest, HeartbeatResponse, InsertRequest, InsertResponse,
        JoinRequest, JoinResponse, LeaveRequest, LeaveResponse, Node, NodeStatus as ProtoNodeStatus,
        ReplicateRequest, ReplicateResponse, SearchRequest, SearchResponse,
        Topology as ProtoTopology, TopologyRequest, TopologyResponse,
    },
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

/// Cluster gRPC server
pub struct ClusterGrpcServer {
    /// Shared cluster manager
    cluster: Arc<RwLock<ClusterManager>>,
    /// Server bind address
    bind_addr: SocketAddr,
}

impl ClusterGrpcServer {
    /// Create new gRPC server
    pub fn new(cluster: Arc<RwLock<ClusterManager>>, bind_addr: SocketAddr) -> Self {
        Self {
            cluster,
            bind_addr,
        }
    }

    /// Start the gRPC server
    pub async fn start(&self) -> crate::Result<()> {
        let service = ClusterServiceImpl {
            cluster: self.cluster.clone(),
        };

        let addr = self.bind_addr;
        tracing::info!("Starting cluster gRPC server on {}", addr);

        tonic::transport::Server::builder()
            .add_service(ClusterServiceServer::new(service))
            .serve(addr)
            .await
            .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

        Ok(())
    }
}

/// gRPC service implementation
#[derive(Clone)]
struct ClusterServiceImpl {
    cluster: Arc<RwLock<ClusterManager>>,
}

#[tonic::async_trait]
impl ClusterService for ClusterServiceImpl {
    /// Handle node join request
    async fn join_cluster(
        &self,
        request: Request<JoinRequest>,
    ) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Node {} joining cluster from {}", req.node_id, req.address);

        let node = NodeInfo {
            id: req.node_id.clone(),
            address: req.address.parse().map_err(|e| {
                Status::invalid_argument(format!("Invalid address: {}", e))
            })?,
            status: NodeStatus::Active,
            shards: vec![],
            capacity: req.capacity as usize,
            load: 0,
            last_heartbeat: current_timestamp(),
        };

        // Add node to cluster
        self.cluster.write().await.add_node(node);

        // Get current topology
        let topology = {
            let cluster = self.cluster.read().await;
            build_topology(&cluster)
        };

        Ok(Response::new(JoinResponse {
            success: true,
            error: String::new(),
            topology: Some(topology),
        }))
    }

    /// Handle node leave request
    async fn leave_cluster(
        &self,
        request: Request<LeaveRequest>,
    ) -> Result<Response<LeaveResponse>, Status> {
        let req = request.into_inner();
        tracing::info!("Node {} leaving cluster", req.node_id);

        self.cluster.write().await.remove_node(&req.node_id);

        Ok(Response::new(LeaveResponse { success: true }))
    }

    /// Handle topology request
    async fn get_topology(
        &self,
        _request: Request<TopologyRequest>,
    ) -> Result<Response<TopologyResponse>, Status> {
        let cluster = self.cluster.read().await;
        let topology = build_topology(&cluster);

        Ok(Response::new(TopologyResponse {
            topology: Some(topology),
        }))
    }

    /// Handle heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        
        // Update node heartbeat in topology
        // This would be implemented in ClusterManager
        tracing::debug!("Heartbeat from node {} at {}", req.node_id, req.timestamp);

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            server_timestamp: current_timestamp(),
        }))
    }

    /// Handle search request (forwarded from another node)
    async fn search(
        &self,
        request: Request<SearchRequest>,
    ) -> Result<Response<SearchResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("Received forwarded search for collection {}", req.collection);

        // TODO: Execute search on local node and return results
        // This requires integration with CollectionManager

        Ok(Response::new(SearchResponse {
            results: vec![],
            error: String::new(),
        }))
    }

    /// Handle insert request (forwarded from another node)
    async fn insert(
        &self,
        request: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("Received forwarded insert for collection {}", req.collection);

        // TODO: Execute insert on local node
        // This requires integration with CollectionManager

        Ok(Response::new(InsertResponse {
            success: true,
            error: String::new(),
        }))
    }

    /// Handle replication request
    async fn replicate(
        &self,
        request: Request<ReplicateRequest>,
    ) -> Result<Response<ReplicateResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!("Received replication for {}:{}", req.collection, req.id);

        // TODO: Store replicated data
        // This requires integration with storage layer

        Ok(Response::new(ReplicateResponse {
            success: true,
            replicated_at: current_timestamp(),
        }))
    }
}

/// Build protobuf topology from cluster manager
fn build_topology(cluster: &ClusterManager) -> ProtoTopology {
    let active_nodes = cluster.active_nodes();
    
    ProtoTopology {
        version: cluster.topology_version(),
        nodes: active_nodes.iter().map(|n| Node {
            id: n.id.clone(),
            address: n.address.to_string(),
            status: node_status_to_proto(n.status) as i32,
            capacity: n.capacity as u64,
            load: n.load as u64,
            shards: n.shards.clone(),
        }).collect(),
        shard_mapping: vec![], // TODO: Populate shard mappings
    }
}

/// Convert internal NodeStatus to protobuf
fn node_status_to_proto(status: NodeStatus) -> ProtoNodeStatus {
    match status {
        NodeStatus::Joining => ProtoNodeStatus::Joining,
        NodeStatus::Active => ProtoNodeStatus::Active,
        NodeStatus::Suspect => ProtoNodeStatus::Suspect,
        NodeStatus::Offline => ProtoNodeStatus::Offline,
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
    fn test_node_status_conversion() {
        assert_eq!(
            node_status_to_proto(NodeStatus::Active) as i32,
            ProtoNodeStatus::Active as i32
        );
        assert_eq!(
            node_status_to_proto(NodeStatus::Offline) as i32,
            ProtoNodeStatus::Offline as i32
        );
    }
}
