//! Query Intelligence Engine
//! 
//! Intent classification, query expansion, and decomposition

use super::{QueryIntent, SmartSearchRequest};
use crate::{Result, SearchRequest};

/// Query Intelligence Engine
pub struct QueryIntelligence {
    /// Intent patterns
    patterns: IntentPatterns,
}

/// Intent detection patterns
struct IntentPatterns {
    /// Question words for factual queries
    factual_words: Vec<&'static str>,
    /// Comparison indicators
    comparison_words: Vec<&'static str>,
    /// Procedural indicators
    procedural_words: Vec<&'static str>,
    /// Temporal words
    temporal_words: Vec<&'static str>,
}

impl Default for IntentPatterns {
    fn default() -> Self {
        Self {
            factual_words: vec!["what", "who", "where", "which", "how many", "how much"],
            comparison_words: vec!["compare", "vs", "versus", "difference", "similarities", "better", "worse"],
            procedural_words: vec!["how to", "how do", "steps", "guide", "tutorial"],
            temporal_words: vec!["when", "date", "year", "time", "recent", "latest"],
        }
    }
}

impl QueryIntelligence {
    /// Create new query intelligence engine
    pub fn new() -> Self {
        Self {
            patterns: IntentPatterns::default(),
        }
    }

    /// Analyze query and create smart request
    pub fn analyze(&self, request: SearchRequest) -> Result<SmartSearchRequest> {
        // TODO: Convert vector query to text for analysis
        // For now, use simple intent detection
        
        let intent = QueryIntent::Unknown;
        let expansions = Vec::new();
        let plan = None;

        Ok(SmartSearchRequest {
            base: request,
            intent,
            expansions,
            plan,
        })
    }

    /// Classify query intent from text
    fn classify_intent(&self, text: &str) -> QueryIntent {
        let lower = text.to_lowercase();

        // Check for procedural
        for word in &self.patterns.procedural_words {
            if lower.contains(word) {
                return QueryIntent::Procedural;
            }
        }

        // Check for comparison
        for word in &self.patterns.comparison_words {
            if lower.contains(word) {
                return QueryIntent::Comparative;
            }
        }

        // Check for temporal
        for word in &self.patterns.temporal_words {
            if lower.contains(word) {
                return QueryIntent::Temporal;
            }
        }

        // Check for factual
        for word in &self.patterns.factual_words {
            if lower.contains(word) {
                return QueryIntent::Factual;
            }
        }

        QueryIntent::Unknown
    }
}

impl Default for QueryIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

/// Query Expander using thesaurus and co-occurrence
pub struct QueryExpander {
    /// Synonym dictionary
    synonyms: std::collections::HashMap<String, Vec<String>>,
}

impl QueryExpander {
    /// Create new expander
    pub fn new() -> Self {
        let mut synonyms = std::collections::HashMap::new();
        
        // Add some common synonyms
        synonyms.insert("fast".to_string(), vec!["quick".to_string(), "rapid".to_string(), "speedy".to_string()]);
        synonyms.insert("big".to_string(), vec!["large".to_string(), "huge".to_string(), "massive".to_string()]);
        
        Self { synonyms }
    }

    /// Expand query
    pub fn expand(&self, query: &str) -> Vec<String> {
        let mut expansions = vec![query.to_string()];
        
        for (word, syns) in &self.synonyms {
            if query.to_lowercase().contains(word) {
                for syn in syns {
                    let expanded = query.to_lowercase().replace(word, syn);
                    expansions.push(expanded);
                }
            }
        }
        
        expansions
    }
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::new()
    }
}
