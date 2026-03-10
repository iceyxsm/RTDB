//! Knowledge Graph (Rule-Based)
//! 
//! Entity extraction and relation extraction without ML

use crate::VectorId;
use std::collections::{HashMap, HashSet};

/// Knowledge Graph
pub struct KnowledgeGraph {
    /// Entities
    entities: HashMap<String, Entity>,
    /// Relations (edges)
    relations: Vec<Relation>,
    /// Entity mentions by document
    mentions: HashMap<VectorId, Vec<String>>,
}

/// Entity node
#[derive(Debug, Clone)]
pub struct Entity {
    /// Entity ID
    pub id: String,
    /// Entity type
    pub type_: EntityType,
    /// Canonical name
    pub name: String,
    /// Aliases
    pub aliases: Vec<String>,
}

/// Entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Concept,
    Product,
    Event,
    Unknown,
}

/// Relation (edge)
#[derive(Debug, Clone)]
pub struct Relation {
    /// Subject entity ID
    pub subject: String,
    /// Predicate
    pub predicate: PredicateType,
    /// Object entity ID
    pub object: String,
    /// Source document
    pub source: VectorId,
}

/// Predicate types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateType {
    IsA,
    PartOf,
    LocatedIn,
    WorksFor,
    Creates,
    Causes,
    Mentions,
}

impl KnowledgeGraph {
    /// Create empty knowledge graph
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
            mentions: HashMap::new(),
        }
    }

    /// Extract entities from text
    pub fn extract_entities(&mut self, text: &str, doc_id: VectorId) -> Vec<String> {
        let mut found = Vec::new();
        
        // Simple pattern-based extraction (capitalized phrases)
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut i = 0;
        
        while i < words.len() {
            // Check if word is capitalized (potential entity start)
            if let Some(word) = words.get(i) {
                if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    // Collect consecutive capitalized words
                    let mut entity_words = vec![*word];
                    i += 1;
                    
                    while i < words.len() {
                        if let Some(next) = words.get(i) {
                            if next.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                entity_words.push(*next);
                                i += 1;
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    let entity_name = entity_words.join(" ");
                    let id = format!("entity_{}", self.entities.len());
                    
                    self.entities.insert(id.clone(), Entity {
                        id: id.clone(),
                        type_: EntityType::Unknown,
                        name: entity_name.clone(),
                        aliases: Vec::new(),
                    });
                    
                    found.push(id);
                    continue;
                }
            }
            i += 1;
        }
        
        self.mentions.insert(doc_id, found.clone());
        found
    }

    /// Add relation
    pub fn add_relation(&mut self, relation: Relation) {
        self.relations.push(relation);
    }

    /// Find relations for entity
    pub fn find_relations(&self, entity_id: &str) -> Vec<&Relation> {
        self.relations.iter()
            .filter(|r| r.subject == entity_id || r.object == entity_id)
            .collect()
    }

    /// Find path between entities
    pub fn find_path(&self, from: &str, to: &str, max_depth: usize) -> Option<Vec<&Relation>> {
        // Simple BFS
        let mut visited = HashSet::new();
        let mut queue = vec![(from, Vec::new())];
        
        while let Some((current, path)) = queue.pop() {
            if current == to {
                return Some(path);
            }
            
            if visited.insert(current) && path.len() < max_depth {
                for rel in self.find_relations(current) {
                    let next = if rel.subject == current {
                        &rel.object
                    } else {
                        &rel.subject
                    };
                    
                    let mut new_path = path.clone();
                    new_path.push(rel);
                    queue.push((next, new_path));
                }
            }
        }
        
        None
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// PageRank calculator for importance scoring
pub struct PageRank {
    /// Damping factor
    damping: f64,
    /// Number of iterations
    iterations: usize,
}

impl PageRank {
    /// Create PageRank calculator
    pub fn new(damping: f64, iterations: usize) -> Self {
        Self { damping, iterations }
    }

    /// Calculate PageRank scores
    pub fn calculate(&self, graph: &KnowledgeGraph) -> HashMap<String, f64> {
        let n = graph.entities.len() as f64;
        let mut scores: HashMap<String, f64> = graph.entities.keys()
            .map(|k| (k.clone(), 1.0 / n))
            .collect();

        for _ in 0..self.iterations {
            let mut new_scores = HashMap::new();

            for entity_id in graph.entities.keys() {
                let mut rank = (1.0 - self.damping) / n;

                // Sum contributions from incoming links
                for rel in &graph.relations {
                    if rel.object == *entity_id {
                        if let Some(&score) = scores.get(&rel.subject) {
                            // Count outgoing links from subject
                            let out_count = graph.relations.iter()
                                .filter(|r| r.subject == rel.subject)
                                .count() as f64;
                            
                            if out_count > 0.0 {
                                rank += self.damping * score / out_count;
                            }
                        }
                    }
                }

                new_scores.insert(entity_id.clone(), rank);
            }

            scores = new_scores;
        }

        scores
    }
}
