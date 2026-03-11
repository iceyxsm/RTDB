//! Query Intelligence Engine
//! 
//! Intent classification, query expansion, and decomposition
//! without using AI/ML models - using pattern matching, statistics, and heuristics

use super::{QueryIntent, QueryPlan, SmartSearchRequest, StepType};
use crate::{Result, SearchRequest};

/// Query Intelligence Engine
pub struct QueryIntelligence {
    /// Intent patterns
    patterns: IntentPatterns,
}

/// Intent detection patterns
#[derive(Debug, Clone)]
struct IntentPatterns {
    /// Question words for factual queries
    factual_words: Vec<String>,
    /// Comparison indicators
    comparison_words: Vec<String>,
    /// Procedural indicators
    procedural_words: Vec<String>,
    /// Temporal words
    temporal_words: Vec<String>,
    /// Causal words
    causal_words: Vec<String>,
    /// Definitional patterns
    definitional_patterns: Vec<String>,
    /// Aggregational patterns
    aggregational_patterns: Vec<String>,
}

impl IntentPatterns {
    fn new() -> Self {
        Self {
            factual_words: vec![
                "what".to_string(),
                "who".to_string(),
                "where".to_string(),
                "which".to_string(),
                "how many".to_string(),
                "how much".to_string(),
            ],
            comparison_words: vec![
                "compare".to_string(),
                "vs".to_string(),
                "versus".to_string(),
                "difference".to_string(),
                "similarities".to_string(),
                "better".to_string(),
                "worse".to_string(),
                "similar to".to_string(),
            ],
            procedural_words: vec![
                "how to".to_string(),
                "how do".to_string(),
                "steps".to_string(),
                "guide".to_string(),
                "tutorial".to_string(),
                "process".to_string(),
                "procedure".to_string(),
            ],
            temporal_words: vec![
                "when".to_string(),
                "date".to_string(),
                "year".to_string(),
                "time".to_string(),
                "recent".to_string(),
                "latest".to_string(),
                "current".to_string(),
                "history".to_string(),
            ],
            causal_words: vec![
                "why".to_string(),
                "cause".to_string(),
                "reason".to_string(),
                "because".to_string(),
                "leads to".to_string(),
                "results in".to_string(),
            ],
            definitional_patterns: vec![
                "what is".to_string(),
                "what are".to_string(),
                "define".to_string(),
                "definition of".to_string(),
                "meaning of".to_string(),
            ],
            aggregational_patterns: vec![
                "list".to_string(),
                "all".to_string(),
                "every".to_string(),
                "examples of".to_string(),
                "types of".to_string(),
            ],
        }
    }
}

impl QueryIntelligence {
    /// Create new query intelligence engine
    pub fn new() -> Self {
        Self {
            patterns: IntentPatterns::new(),
        }
    }

    /// Analyze query text and detect intent
    pub fn analyze_text(&self, text: &str) -> QueryIntent {
        let lower = text.to_lowercase();

        // Check for definitional (highest priority for "what is" patterns)
        for pattern in &self.patterns.definitional_patterns {
            if lower.starts_with(pattern) {
                return QueryIntent::Definitional;
            }
        }

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

        // Check for causal
        for word in &self.patterns.causal_words {
            if lower.contains(word) || lower.starts_with(word) {
                return QueryIntent::Causal;
            }
        }

        // Check for aggregational
        for pattern in &self.patterns.aggregational_patterns {
            if lower.starts_with(pattern) {
                return QueryIntent::Aggregational;
            }
        }

        // Check for temporal
        for word in &self.patterns.temporal_words {
            if lower.starts_with(word) || lower.contains(word) {
                return QueryIntent::Temporal;
            }
        }

        // Check for factual (question words)
        for word in &self.patterns.factual_words {
            if lower.starts_with(word) {
                return QueryIntent::Factual;
            }
        }

        // Check if it's a statement vs question
        if lower.ends_with('?') {
            QueryIntent::Factual
        } else {
            QueryIntent::Unknown
        }
    }

    /// Extract entities from text using simple heuristics
    pub fn extract_entities(&self, text: &str) -> Vec<String> {
        let mut entities = Vec::new();
        // Strip punctuation for word boundaries but preserve original for extraction
        let clean_text: String = text.chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' }).collect();
        let words: Vec<&str> = clean_text.split_whitespace().collect();

        // Look for capitalized phrases (potential named entities)
        let mut i = 0;
        while i < words.len() {
            let word = words[i];
            
            // Check if word starts with uppercase and is not a sentence start
            if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) 
                && i > 0 
                && !word.chars().all(|c| c.is_uppercase()) { // Not all caps
                
                // Collect consecutive capitalized words
                let mut entity_words = vec![word];
                i += 1;
                
                while i < words.len() {
                    let next = words[i];
                    // Check if next word is also capitalized or a common connector
                    if next.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                        || next == "of" || next == "the" || next == "and" {
                        entity_words.push(next);
                        i += 1;
                    } else {
                        break;
                    }
                }
                
                let entity = entity_words.join(" ");
                if entity.len() > 2 { // Filter out single letters
                    entities.push(entity);
                }
                continue;
            }
            i += 1;
        }

        entities
    }

    /// Determine if query requires multi-hop reasoning
    pub fn is_multi_hop(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        
        // Patterns indicating multi-hop needs
        let multi_hop_indicators = [
            "of the",
            "from the",
            "by the",
            "in the",
            "who was",
            "what was",
            "which was",
        ];
        
        // Check for nested references
        for indicator in &multi_hop_indicators {
            if lower.contains(indicator) {
                return true;
            }
        }
        
        // Check for multiple entities
        let entities = self.extract_entities(text);
        entities.len() > 2
    }

    /// Create query plan for multi-hop queries
    pub fn create_plan(&self, text: &str, intent: QueryIntent) -> Option<QueryPlan> {
        if !self.is_multi_hop(text) {
            return None;
        }

        let entities = self.extract_entities(text);
        if entities.len() < 2 {
            return None;
        }

        let steps = match intent {
            QueryIntent::Comparative => {
                vec![
                    super::QueryStep {
                        query: entities[0].clone(),
                        dependencies: vec![],
                        step_type: StepType::Retrieve,
                    },
                    super::QueryStep {
                        query: entities[1].clone(),
                        dependencies: vec![],
                        step_type: StepType::Retrieve,
                    },
                    super::QueryStep {
                        query: format!("Compare {} vs {}", entities[0], entities[1]),
                        dependencies: vec![0, 1],
                        step_type: StepType::Aggregate,
                    },
                ]
            }
            _ => {
                // Chain of retrievals
                entities.iter().enumerate()
                    .map(|(i, entity)| super::QueryStep {
                        query: entity.clone(),
                        dependencies: if i > 0 { vec![i - 1] } else { vec![] },
                        step_type: StepType::Retrieve,
                    })
                    .collect()
            }
        };

        Some(QueryPlan { steps })
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
    /// Create new expander with common synonyms
    pub fn new() -> Self {
        let mut synonyms = std::collections::HashMap::new();
        
        // Add common synonyms
        synonyms.insert("fast".to_string(), vec![
            "quick".to_string(),
            "rapid".to_string(),
            "speedy".to_string(),
            "swift".to_string(),
        ]);
        
        synonyms.insert("big".to_string(), vec![
            "large".to_string(),
            "huge".to_string(),
            "massive".to_string(),
            "enormous".to_string(),
        ]);
        
        synonyms.insert("important".to_string(), vec![
            "significant".to_string(),
            "crucial".to_string(),
            "critical".to_string(),
            "essential".to_string(),
        ]);
        
        synonyms.insert("good".to_string(), vec![
            "excellent".to_string(),
            "great".to_string(),
            "superior".to_string(),
            "fine".to_string(),
        ]);
        
        synonyms.insert("start".to_string(), vec![
            "begin".to_string(),
            "initiate".to_string(),
            "commence".to_string(),
            "launch".to_string(),
        ]);
        
        Self { synonyms }
    }

    /// Expand query with synonyms
    pub fn expand(&self, query: &str) -> Vec<String> {
        let mut expansions = vec![query.to_string()];
        let lower_query = query.to_lowercase();
        
        // Split into words and check each
        for (word, syns) in &self.synonyms {
            // Check if word appears in query
            if lower_query.contains(word) {
                // Create expansion with each synonym
                for syn in syns {
                    let expanded = lower_query.replace(word, syn);
                    if !expansions.contains(&expanded) {
                        expansions.push(expanded);
                    }
                }
            }
        }
        
        // Limit expansions to avoid explosion
        expansions.truncate(5);
        expansions
    }

    /// Generate morphological variations
    pub fn morphological_variations(&self, word: &str) -> Vec<String> {
        let mut variations = vec![word.to_string()];
        let lower = word.to_lowercase();
        
        // Simple stemming rules
        if lower.ends_with("ing") {
            variations.push(lower[..lower.len()-3].to_string());
        }
        if lower.ends_with("ed") {
            variations.push(lower[..lower.len()-2].to_string());
        }
        if lower.ends_with("s") && lower.len() > 1 {
            variations.push(lower[..lower.len()-1].to_string());
        }
        if lower.ends_with("es") && lower.len() > 2 {
            variations.push(lower[..lower.len()-2].to_string());
        }
        
        variations
    }
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_classification() {
        let qi = QueryIntelligence::new();

        assert_eq!(qi.analyze_text("What is the capital of France?"), QueryIntent::Definitional);
        assert_eq!(qi.analyze_text("How to cook pasta?"), QueryIntent::Procedural);
        assert_eq!(qi.analyze_text("Compare apples vs oranges"), QueryIntent::Comparative);
        assert_eq!(qi.analyze_text("When did WW2 start?"), QueryIntent::Temporal);
        assert_eq!(qi.analyze_text("Why is the sky blue?"), QueryIntent::Causal);
        assert_eq!(qi.analyze_text("List all countries in Europe"), QueryIntent::Aggregational);
    }

    #[test]
    fn test_entity_extraction() {
        let qi = QueryIntelligence::new();
        
        let entities = qi.extract_entities("What is the capital of United States?");
        assert!(entities.contains(&"United States".to_string()));
        
        let entities = qi.extract_entities("Who founded Microsoft Corporation?");
        assert!(entities.contains(&"Microsoft Corporation".to_string()));
    }

    #[test]
    fn test_query_expansion() {
        let expander = QueryExpander::new();
        
        let expansions = expander.expand("fast car");
        assert!(expansions.contains(&"fast car".to_string()));
        assert!(expansions.contains(&"quick car".to_string()));
        assert!(expansions.contains(&"rapid car".to_string()));
    }
}
