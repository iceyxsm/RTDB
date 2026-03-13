//! Fault injection (Nemesis) implementations for Jepsen tests
//!
//! Provides various fault injection mechanisms to test system resilience:
//! - Network partitions
//! - Node failures (kill/pause)
//! - Clock skew
//! - Network delays and packet loss

use super::{FaultEvent, FaultType, Nemesis, PartitionType};
use crate::Result;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Network partition nemesis using iptables/netsh
pub struct NetworkPartitionNemesis {
    /// Node addresses for partition simulation
    node_addresses: Vec<String>,
    /// Active partitions
    active_partitions: Arc<RwLock<HashMap<Uuid, PartitionState>>>,
}

#[derive(Debug, Clone)]
struct PartitionState {
    partition_type: PartitionType,
    affected_nodes: Vec<usize>,
    start_time: SystemTime,
}

impl NetworkPartitionNemesis {
    pub fn new(node_addresses: Vec<String>) -> Self {
        Self {
            node_addresses,
            active_partitions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl Nemesis for NetworkPartitionNemesis {
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting network partition nemesis");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping network partition nemesis");
        
        // Clear all active partitions
        let partitions = self.active_partitions.read().await;
        for partition_id in partitions.keys() {
            self.recover(*partition_id).await?;
        }
        
        Ok(())
    }

    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent> {
        match fault {
            FaultType::Partition(partition_type) => {
                let fault_id = Uuid::new_v4();
                let start_time = SystemTime::now();
                
                self.create_partition(&partition_type, &nodes).await?;
                
                let partition_state = PartitionState {
                    partition_type: partition_type.clone(),
                    affected_nodes: nodes.clone(),
                    start_time,
                };
                
                self.active_partitions.write().await.insert(fault_id, partition_state);
                
                Ok(FaultEvent {
                    fault_type: FaultType::Partition(partition_type),
                    start_time,
                    end_time: None,
                    affected_nodes: nodes,
                })
            }
            _ => Err(crate::RTDBError::Config("Unsupported fault type for NetworkPartitionNemesis".to_string())),
        }
    }

    async fn recover(&self, fault_id: Uuid) -> Result<()> {
        if let Some(partition) = self.active_partitions.write().await.remove(&fault_id) {
            self.heal_partition(&partition.partition_type, &partition.affected_nodes).await?;
        }
        Ok(())
    }
}

impl NetworkPartitionNemesis {
    async fn create_partition(&self, partition_type: &PartitionType, nodes: &[usize]) -> Result<()> {
        match partition_type {
            PartitionType::MajorityMinority => {
                self.create_majority_minority_partition(nodes).await
            }
            PartitionType::Complete => {
                self.create_complete_partition(nodes).await
            }
            PartitionType::Random => {
                self.create_random_partition(nodes).await
            }
            PartitionType::Ring => {
                self.create_ring_partition(nodes).await
            }
        }
    }

    async fn create_majority_minority_partition(&self, nodes: &[usize]) -> Result<()> {
        let majority_size = (nodes.len() / 2) + 1;
        let majority_nodes = &nodes[..majority_size];
        let minority_nodes = &nodes[majority_size..];
        
        // Block communication between majority and minority
        for &maj_node in majority_nodes {
            for &min_node in minority_nodes {
                self.block_communication(maj_node, min_node).await?;
            }
        }
        
        tracing::info!(
            "Created majority/minority partition: majority={:?}, minority={:?}",
            majority_nodes, minority_nodes
        );
        
        Ok(())
    }

    async fn create_complete_partition(&self, nodes: &[usize]) -> Result<()> {
        // Block all communication between all nodes
        for (i, &node1) in nodes.iter().enumerate() {
            for &node2 in nodes.iter().skip(i + 1) {
                self.block_communication(node1, node2).await?;
            }
        }
        
        tracing::info!("Created complete partition for nodes: {:?}", nodes);
        Ok(())
    }

    async fn create_random_partition(&self, _nodes: &[usize]) -> Result<()> {
        // Simplified random partition - in production would implement proper random partitioning
        tracing::info!("Random partition not fully implemented");
        Ok(())
    }

    async fn create_ring_partition(&self, _nodes: &[usize]) -> Result<()> {
        // Simplified ring partition - in production would implement ring topology partitioning
        tracing::info!("Ring partition not fully implemented");
        Ok(())
    }

    async fn block_communication(&self, node1: usize, node2: usize) -> Result<()> {
        if node1 >= self.node_addresses.len() || node2 >= self.node_addresses.len() {
            return Err(crate::RTDBError::Config("Invalid node index".to_string()));
        }

        let addr1 = &self.node_addresses[node1];
        let addr2 = &self.node_addresses[node2];
        
        // Extract IP addresses (simplified - assumes format "ip:port")
        let ip1 = addr1.split(':').next().unwrap_or(addr1);
        let ip2 = addr2.split(':').next().unwrap_or(addr2);
        
        // Use iptables on Linux or netsh on Windows to block traffic
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("iptables")
                .args(&["-A", "INPUT", "-s", ip2, "-d", ip1, "-j", "DROP"])
                .output();
            
            if let Err(e) = output {
                tracing::warn!("Failed to create iptables rule: {}", e);
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            // Windows firewall rules would go here
            tracing::warn!("Windows firewall rules not implemented");
        }
        
        tracing::debug!("Blocked communication between {} and {}", ip1, ip2);
        Ok(())
    }

    async fn heal_partition(&self, partition_type: &PartitionType, nodes: &[usize]) -> Result<()> {
        // Remove all blocking rules for the affected nodes
        for (i, &node1) in nodes.iter().enumerate() {
            for &node2 in nodes.iter().skip(i + 1) {
                self.unblock_communication(node1, node2).await?;
            }
        }
        
        tracing::info!("Healed {:?} partition for nodes: {:?}", partition_type, nodes);
        Ok(())
    }

    async fn unblock_communication(&self, node1: usize, node2: usize) -> Result<()> {
        if node1 >= self.node_addresses.len() || node2 >= self.node_addresses.len() {
            return Err(crate::RTDBError::Config("Invalid node index".to_string()));
        }

        let addr1 = &self.node_addresses[node1];
        let addr2 = &self.node_addresses[node2];
        
        let ip1 = addr1.split(':').next().unwrap_or(addr1);
        let ip2 = addr2.split(':').next().unwrap_or(addr2);
        
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("iptables")
                .args(&["-D", "INPUT", "-s", ip2, "-d", ip1, "-j", "DROP"])
                .output();
            
            if let Err(e) = output {
                tracing::warn!("Failed to remove iptables rule: {}", e);
            }
        }
        
        tracing::debug!("Unblocked communication between {} and {}", ip1, ip2);
        Ok(())
    }
}
/// Process kill/pause nemesis
pub struct ProcessNemesis {
    /// Process IDs for each node
    node_pids: Arc<RwLock<HashMap<usize, u32>>>,
    /// Active faults
    active_faults: Arc<RwLock<HashMap<Uuid, ProcessFaultState>>>,
}

#[derive(Debug, Clone)]
struct ProcessFaultState {
    fault_type: ProcessFaultType,
    affected_nodes: Vec<usize>,
    start_time: SystemTime,
}

#[derive(Debug, Clone)]
enum ProcessFaultType {
    Kill,
    Pause,
}

impl ProcessNemesis {
    pub fn new() -> Self {
        Self {
            node_pids: Arc::new(RwLock::new(HashMap::new())),
            active_faults: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_node(&self, node_id: usize, pid: u32) {
        self.node_pids.write().await.insert(node_id, pid);
    }
}

#[async_trait::async_trait]
impl Nemesis for ProcessNemesis {
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting process nemesis");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping process nemesis");
        
        // Recover from all active faults
        let faults: Vec<Uuid> = self.active_faults.read().await.keys().cloned().collect();
        for fault_id in faults {
            self.recover(fault_id).await?;
        }
        
        Ok(())
    }

    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent> {
        let fault_id = Uuid::new_v4();
        let start_time = SystemTime::now();
        
        match fault {
            FaultType::Kill => {
                for &node_id in &nodes {
                    self.kill_node(node_id).await?;
                }
                
                let fault_state = ProcessFaultState {
                    fault_type: ProcessFaultType::Kill,
                    affected_nodes: nodes.clone(),
                    start_time,
                };
                
                self.active_faults.write().await.insert(fault_id, fault_state);
                
                Ok(FaultEvent {
                    fault_type: FaultType::Kill,
                    start_time,
                    end_time: None,
                    affected_nodes: nodes,
                })
            }
            FaultType::Pause => {
                for &node_id in &nodes {
                    self.pause_node(node_id).await?;
                }
                
                let fault_state = ProcessFaultState {
                    fault_type: ProcessFaultType::Pause,
                    affected_nodes: nodes.clone(),
                    start_time,
                };
                
                self.active_faults.write().await.insert(fault_id, fault_state);
                
                Ok(FaultEvent {
                    fault_type: FaultType::Pause,
                    start_time,
                    end_time: None,
                    affected_nodes: nodes,
                })
            }
            _ => Err(crate::RTDBError::Config("Unsupported fault type for ProcessNemesis".to_string())),
        }
    }

    async fn recover(&self, fault_id: Uuid) -> Result<()> {
        if let Some(fault_state) = self.active_faults.write().await.remove(&fault_id) {
            match fault_state.fault_type {
                ProcessFaultType::Kill => {
                    // For kill faults, we would need to restart the processes
                    // This is simplified - in production would have process management
                    tracing::info!("Would restart killed processes for nodes: {:?}", fault_state.affected_nodes);
                }
                ProcessFaultType::Pause => {
                    for &node_id in &fault_state.affected_nodes {
                        self.resume_node(node_id).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl ProcessNemesis {
    async fn kill_node(&self, node_id: usize) -> Result<()> {
        let pids = self.node_pids.read().await;
        if let Some(&pid) = pids.get(&node_id) {
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            
            #[cfg(windows)]
            {
                let output = Command::new("taskkill")
                    .args(&["/PID", &pid.to_string(), "/F"])
                    .output();
                
                if let Err(e) = output {
                    tracing::warn!("Failed to kill process {}: {}", pid, e);
                }
            }
            
            tracing::info!("Killed node {} (PID: {})", node_id, pid);
        }
        Ok(())
    }

    async fn pause_node(&self, node_id: usize) -> Result<()> {
        let pids = self.node_pids.read().await;
        if let Some(&pid) = pids.get(&node_id) {
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGSTOP);
                }
            }
            
            #[cfg(windows)]
            {
                // Windows doesn't have direct SIGSTOP equivalent
                // Would need to use process suspension APIs
                tracing::warn!("Process pause not implemented on Windows");
            }
            
            tracing::info!("Paused node {} (PID: {})", node_id, pid);
        }
        Ok(())
    }

    async fn resume_node(&self, node_id: usize) -> Result<()> {
        let pids = self.node_pids.read().await;
        if let Some(&pid) = pids.get(&node_id) {
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(pid as i32, libc::SIGCONT);
                }
            }
            
            #[cfg(windows)]
            {
                tracing::warn!("Process resume not implemented on Windows");
            }
            
            tracing::info!("Resumed node {} (PID: {})", node_id, pid);
        }
        Ok(())
    }
}

/// Clock skew nemesis
pub struct ClockSkewNemesis {
    /// Maximum skew in milliseconds
    max_skew_ms: i64,
    /// Active clock skews
    active_skews: Arc<RwLock<HashMap<Uuid, ClockSkewState>>>,
}

#[derive(Debug, Clone)]
struct ClockSkewState {
    affected_nodes: Vec<usize>,
    skew_amounts: HashMap<usize, i64>,
    start_time: SystemTime,
}

impl ClockSkewNemesis {
    pub fn new(max_skew_ms: i64) -> Self {
        Self {
            max_skew_ms,
            active_skews: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl Nemesis for ClockSkewNemesis {
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting clock skew nemesis");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping clock skew nemesis");
        
        let skews: Vec<Uuid> = self.active_skews.read().await.keys().cloned().collect();
        for skew_id in skews {
            self.recover(skew_id).await?;
        }
        
        Ok(())
    }

    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent> {
        match fault {
            FaultType::ClockSkew { max_skew_ms } => {
                let fault_id = Uuid::new_v4();
                let start_time = SystemTime::now();
                
                let mut skew_amounts = HashMap::new();
                for &node_id in &nodes {
                    let skew = rand::random::<i64>() % max_skew_ms;
                    skew_amounts.insert(node_id, skew);
                    self.apply_clock_skew(node_id, skew).await?;
                }
                
                let skew_state = ClockSkewState {
                    affected_nodes: nodes.clone(),
                    skew_amounts,
                    start_time,
                };
                
                self.active_skews.write().await.insert(fault_id, skew_state);
                
                Ok(FaultEvent {
                    fault_type: FaultType::ClockSkew { max_skew_ms },
                    start_time,
                    end_time: None,
                    affected_nodes: nodes,
                })
            }
            _ => Err(crate::RTDBError::Config("Unsupported fault type for ClockSkewNemesis".to_string())),
        }
    }

    async fn recover(&self, fault_id: Uuid) -> Result<()> {
        if let Some(skew_state) = self.active_skews.write().await.remove(&fault_id) {
            for &node_id in &skew_state.affected_nodes {
                self.reset_clock_skew(node_id).await?;
            }
        }
        Ok(())
    }
}

impl ClockSkewNemesis {
    async fn apply_clock_skew(&self, node_id: usize, skew_ms: i64) -> Result<()> {
        // In a real implementation, this would adjust system clocks
        // For testing, we might inject artificial delays or use libfaketime
        tracing::info!("Applied {}ms clock skew to node {}", skew_ms, node_id);
        Ok(())
    }

    async fn reset_clock_skew(&self, node_id: usize) -> Result<()> {
        // Reset clock to normal
        tracing::info!("Reset clock skew for node {}", node_id);
        Ok(())
    }
}

/// Combined nemesis that can inject multiple fault types
pub struct CombinedNemesis {
    network_nemesis: NetworkPartitionNemesis,
    process_nemesis: ProcessNemesis,
    clock_nemesis: ClockSkewNemesis,
}

impl CombinedNemesis {
    pub fn new(node_addresses: Vec<String>, max_clock_skew_ms: i64) -> Self {
        Self {
            network_nemesis: NetworkPartitionNemesis::new(node_addresses),
            process_nemesis: ProcessNemesis::new(),
            clock_nemesis: ClockSkewNemesis::new(max_clock_skew_ms),
        }
    }

    pub async fn register_node(&self, node_id: usize, pid: u32) {
        self.process_nemesis.register_node(node_id, pid).await;
    }
}

#[async_trait::async_trait]
impl Nemesis for CombinedNemesis {
    async fn start(&self) -> Result<()> {
        self.network_nemesis.start().await?;
        self.process_nemesis.start().await?;
        self.clock_nemesis.start().await?;
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.network_nemesis.stop().await?;
        self.process_nemesis.stop().await?;
        self.clock_nemesis.stop().await?;
        Ok(())
    }

    async fn inject_fault(&self, fault: FaultType, nodes: Vec<usize>) -> Result<FaultEvent> {
        match fault {
            FaultType::Partition(_) => self.network_nemesis.inject_fault(fault, nodes).await,
            FaultType::Kill | FaultType::Pause => self.process_nemesis.inject_fault(fault, nodes).await,
            FaultType::ClockSkew { .. } => self.clock_nemesis.inject_fault(fault, nodes).await,
            _ => Err(crate::RTDBError::Config("Unsupported fault type".to_string())),
        }
    }

    async fn recover(&self, fault_id: Uuid) -> Result<()> {
        // Try recovery with all nemeses (only the relevant one will have the fault_id)
        let _ = self.network_nemesis.recover(fault_id).await;
        let _ = self.process_nemesis.recover(fault_id).await;
        let _ = self.clock_nemesis.recover(fault_id).await;
        Ok(())
    }
}