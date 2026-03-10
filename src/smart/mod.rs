//! Smart retrieval layer (Zero-AI Intelligence)
//! 
//! Implements algorithmic intelligence without ML models:
//! - Query intent classification
//! - Query expansion
//! - Multi-hop decomposition
//! - Context intelligence
//! - Knowledge graph

pub mod query_intel;
pub mod context;
pub mod knowledge_graph;

use crate::SearchRequest;

/// Smart search request with intelligence
#[derive(Debug, Clone)]
pub struct SmartSearchRequest {
    /// Original search request
    pub base: SearchRequest,
    /// Detected intent
    pub intent: QueryIntent,
    /// Expanded queries
    pub expansions: Vec<String>,
    /// Query plan for complex queries
    pub plan: Option<QueryPlan>,
}

/// Query intent types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Factual lookup
    Factual,
    /// Comparison between entities
    Comparative,
    /// How-to / procedural
    Procedural,
    /// Temporal queries
    Temporal,
    /// Definition
    Definitional,
    /// List/aggregate
    Aggregational,
    /// Causal (why)
    Causal,
    /// Unknown
    Unknown,
}

/// Query plan for multi-hop queries
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Plan steps
    pub steps: Vec<QueryStep>,
}

/// Single query step
#[derive(Debug, Clone)]
pub struct QueryStep {
    /// Step query
    pub query: String,
    /// Dependencies on previous steps
    pub dependencies: Vec<usize>,
    /// Step type
    pub step_type: StepType,
}

/// Step type
#[derive(Debug, Clone)]
pub enum StepType {
    /// Retrieve vectors
    Retrieve,
    /// Filter results
    Filter,
    /// Aggregate results
    Aggregate,
    /// Join results
    Join,
}

/// Smart search result
#[derive(Debug, Clone)]
pub struct SmartSearchResult {
    /// Scored results
    pub results: Vec<crate::ScoredVector>,
    /// Contradictions found
    pub contradictions: Vec<Contradiction>,
    /// Suggested reading order
    pub suggested_order: Vec<usize>,
    /// Confidence scores per result
    pub confidence: Vec<f32>,
}

/// Detected contradiction
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// Result indices that contradict
    pub indices: Vec<usize>,
    /// Contradiction type
    pub type_: ContradictionType,
}

/// Contradiction types
#[derive(Debug, Clone)]
pub enum ContradictionType {
    /// Negation
    Negation,
    /// Numeric mismatch
    Numeric,
    /// Temporal conflict
    Temporal,
}
