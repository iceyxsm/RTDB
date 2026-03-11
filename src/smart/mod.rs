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

/// Smart retrieval engine that orchestrates all smart features
pub struct SmartRetrieval {
    /// Query intelligence
    query_intel: query_intel::QueryIntelligence,
    /// Query expander
    expander: query_intel::QueryExpander,
}

impl SmartRetrieval {
    /// Create new smart retrieval engine
    pub fn new() -> Self {
        Self {
            query_intel: query_intel::QueryIntelligence::new(),
            expander: query_intel::QueryExpander::new(),
        }
    }

    /// Analyze a search request and enhance it with intelligence
    pub fn analyze(&self, request: &SearchRequest) -> SmartSearchRequest {
        // For now, we work with text queries
        // In a real implementation, we'd convert vectors back to text or work with metadata
        
        SmartSearchRequest {
            base: request.clone(),
            intent: QueryIntent::Unknown,
            expansions: Vec::new(),
            plan: None,
        }
    }

    /// Analyze text query
    pub fn analyze_text(&self, text: &str) -> (QueryIntent, Vec<String>, Option<QueryPlan>) {
        let intent = self.query_intel.analyze_text(text);
        let expansions = self.expander.expand(text);
        let plan = self.query_intel.create_plan(text, intent);
        
        (intent, expansions, plan)
    }
}

impl Default for SmartRetrieval {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_retrieval() {
        let sr = SmartRetrieval::new();
        
        // Test text analysis
        let (intent, expansions, _plan) = sr.analyze_text("How to bake bread?");
        assert_eq!(intent, QueryIntent::Procedural);
        assert!(!expansions.is_empty());
    }

    #[test]
    fn test_query_intent_variations() {
        let sr = SmartRetrieval::new();

        // Test various query types
        let test_cases = vec![
            ("What is machine learning?", QueryIntent::Definitional),
            ("Why does it rain?", QueryIntent::Causal),
            ("When was the Eiffel Tower built?", QueryIntent::Temporal),
            ("List all prime numbers", QueryIntent::Aggregational),
            ("Compare Python and Java", QueryIntent::Comparative),
            ("Who is the president?", QueryIntent::Factual),
        ];

        for (query, expected) in test_cases {
            let (intent, _, _) = sr.analyze_text(query);
            assert_eq!(intent, expected, "Failed for query: {}", query);
        }
    }
}
