//! Replication Module
//!
//! This module provides cross-region replication capabilities with
//! conflict resolution and automatic failover.

pub mod cross_region;

pub use cross_region::{
    CrossRegionReplicator,
    VectorClock,
    VectorClockOrdering,
    ReplicatedOperation,
    OperationType,
    ConflictResolution,
    RegionConfig,
    ReplicationStatus,
    ReplicationError,
};