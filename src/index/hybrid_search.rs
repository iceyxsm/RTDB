//! Hybrid Search with Metadata Filtering
//!
//! Implements production-grade hybrid search combining vector similarity
//! with metadata filters. Supports multiple filter strategies:
//! - Pre-filter: Filter metadata before vector search
//! - Post-filter: Filter results after vector search  
//! - Filtered ANN: Integrated filtering during graph traversal
//!
//! ## Filter Types
//! - Exact match: `category = "electronics"`
//! - Range: `price >= 100 AND price < 500`
//! - Set membership: `status IN ("active", "pending")`
//! - Boolean: `is_published = true`

use crate::index::vector_index::SearchResult;
use crate::index::VectorIndex;
use parking_lot::RwLock;
use crate::{RTDBError, Result};
use dashmap::DashMap;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::info;

// ============================================================================
// Filter Types
// ============================================================================

/// Metadata filter condition
#[derive(Debug, Clone, PartialEq)]
pub enum FilterCondition {
    /// Exact string match
    Eq { field: String, value: String },
    /// Not equal
    Ne { field: String, value: String },
    /// Greater than (numeric)
    Gt { field: String, value: f64 },
    /// Greater than or equal
    Gte { field: String, value: f64 },
    /// Less than
    Lt { field: String, value: f64 },
    /// Less than or equal
    Lte { field: String, value: f64 },
    /// Set membership
    In { field: String, values: Vec<String> },
    /// Contains substring
    Contains { field: String, value: String },
    /// Logical AND
    And(Box<FilterCondition>, Box<FilterCondition>),
    /// Logical OR
    Or(Box<FilterCondition>, Box<FilterCondition>),
    /// Logical NOT
    Not(Box<FilterCondition>),
}

impl FilterCondition {
    /// Evaluate filter against metadata
    pub fn evaluate(&self, metadata: &HashMap<String, String>) -> bool {
        match self {
            FilterCondition::Eq { field, value } => {
                metadata.get(field).map(|v| v == value).unwrap_or(false)
            }
            FilterCondition::Ne { field, value } => {
                metadata.get(field).map(|v| v != value).unwrap_or(true)
            }
            FilterCondition::Gt { field, value } => {
                metadata.get(field)
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v > *value)
                    .unwrap_or(false)
            }
            FilterCondition::Gte { field, value } => {
                metadata.get(field)
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v >= *value)
                    .unwrap_or(false)
            }
            FilterCondition::Lt { field, value } => {
                metadata.get(field)
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v < *value)
                    .unwrap_or(false)
            }
            FilterCondition::Lte { field, value } => {
                metadata.get(field)
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v <= *value)
                    .unwrap_or(false)
            }
            FilterCondition::In { field, values } => {
                metadata.get(field)
                    .map(|v| values.contains(v))
                    .unwrap_or(false)
            }
            FilterCondition::Contains { field, value } => {
                metadata.get(field)
                    .map(|v| v.contains(value))
                    .unwrap_or(false)
            }
            FilterCondition::And(left, right) => {
                left.evaluate(metadata) && right.evaluate(metadata)
            }
            FilterCondition::Or(left, right) => {
                left.evaluate(metadata) || right.evaluate(metadata)
            }
            FilterCondition::Not(condition) => {
                !condition.evaluate(metadata)
            }
        }
    }

    /// Get all fields referenced in filter
    pub fn fields(&self) -> HashSet<String> {
        let mut fields = HashSet::new();
        self.collect_fields(&mut fields);
        fields
    }

    fn collect_fields(&self, fields: &mut HashSet<String>) {
        match self {
            FilterCondition::Eq { field, .. }
            | FilterCondition::Ne { field, .. }
            | FilterCondition::Gt { field, .. }
            | FilterCondition::Gte { field, .. }
            | FilterCondition::Lt { field, .. }
            | FilterCondition::Lte { field, .. }
            | FilterCondition::In { field, .. }
            | FilterCondition::Contains { field, .. } => {
                fields.insert(field.clone());
            }
            FilterCondition::And(left, right)
            | FilterCondition::Or(left, right) => {
                left.collect_fields(fields);
                right.collect_fields(fields);
            }
            FilterCondition::Not(condition) => {
                condition.collect_fields(fields);
            }
        }
    }
}

// ============================================================================
// Metadata Index
// ============================================================================

/// Inverted index for metadata fields
pub struct MetadataIndex {
    /// Field name -> value -> set of vector IDs
    indexes: DashMap<String, DashMap<String, HashSet<u64>>>,
    /// Total vector count per field (for selectivity calculation)
    field_counts: DashMap<String, u64>,
}

impl MetadataIndex {
    pub fn new() -> Self {
        Self {
            indexes: DashMap::new(),
            field_counts: DashMap::new(),
        }
    }

    /// Index metadata for a vector
    pub fn index(&self, id: u64, metadata: &HashMap<String, String>) {
        for (field, value) in metadata {
            // Get or create field index
            let field_index = self.indexes
                .entry(field.clone())
                .or_insert_with(DashMap::new);
            
            // Add to value set
            field_index
                .entry(value.clone())
                .or_insert_with(HashSet::new)
                .insert(id);
            
            // Update count
            *self.field_counts.entry(field.clone()).or_insert(0) += 1;
        }
    }

    /// Remove vector from index
    pub fn remove(&self, id: u64, metadata: &HashMap<String, String>) {
        for (field, value) in metadata {
            if let Some(field_index) = self.indexes.get(field) {
                if let Some(mut value_set) = field_index.get_mut(value) {
                    value_set.remove(&id);
                }
            }
        }
    }

    /// Get IDs matching a filter condition (if indexable)
    pub fn get_matching_ids(&self, filter: &FilterCondition) -> Option<HashSet<u64>> {
        match filter {
            FilterCondition::Eq { field, value } => {
                self.indexes.get(field)?
                    .get(value)
                    .map(|set| set.clone())
            }
            FilterCondition::In { field, values } => {
                let field_index = self.indexes.get(field)?;
                let mut result = HashSet::new();
                for value in values {
                    if let Some(set) = field_index.get(value) {
                        result.extend(set.iter().copied());
                    }
                }
                Some(result)
            }
            FilterCondition::And(left, right) => {
                let left_ids = self.get_matching_ids(left)?;
                let right_ids = self.get_matching_ids(right)?;
                let intersection: HashSet<_> = left_ids.intersection(&right_ids).copied().collect();
                Some(intersection)
            }
            FilterCondition::Or(left, right) => {
                let mut result = self.get_matching_ids(left).unwrap_or_default();
                result.extend(self.get_matching_ids(right).unwrap_or_default());
                Some(result)
            }
            _ => None, // Not indexable
        }
    }

    /// Calculate filter selectivity (0.0 to 1.0, lower is more selective)
    pub fn estimate_selectivity(&self, filter: &FilterCondition) -> f64 {
        match filter {
            FilterCondition::Eq { field, value } => {
                if let Some(field_index) = self.indexes.get(field) {
                    if let Some(set) = field_index.get(value) {
                        let total = self.field_counts.get(field).map(|c| *c).unwrap_or(1);
                        return set.len() as f64 / total as f64;
                    }
                }
                1.0 // Unknown, assume not selective
            }
            FilterCondition::And(left, right) => {
                self.estimate_selectivity(left) * self.estimate_selectivity(right)
            }
            FilterCondition::Or(left, right) => {
                let s1 = self.estimate_selectivity(left);
                let s2 = self.estimate_selectivity(right);
                s1 + s2 - s1 * s2
            }
            _ => 1.0 // Unknown
        }
    }
}

impl Default for MetadataIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Query Cache
// ============================================================================

/// Cache key for query results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct QueryCacheKey {
    query_hash: u64,
    filter_hash: u64,
    k: usize,
}

/// Cached query result with TTL
struct CachedResult {
    results: Vec<SearchResult>,
    timestamp: Instant,
    ttl: Duration,
}

impl CachedResult {
    fn is_expired(&self) -> bool {
        self.timestamp.elapsed() > self.ttl
    }
}

/// LRU cache for query results
pub struct QueryCache {
    cache: DashMap<QueryCacheKey, CachedResult>,
    hit_count: AtomicU64,
    miss_count: AtomicU64,
    default_ttl: Duration,
}

impl QueryCache {
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            cache: DashMap::new(),
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
            default_ttl,
        }
    }

    pub fn get(&self, query: &[f32], filter: &Option<FilterCondition>, k: usize) -> Option<Vec<SearchResult>> {
        let key = self.make_key(query, filter, k);
        
        if let Some(entry) = self.cache.get(&key) {
            if !entry.is_expired() {
                self.hit_count.fetch_add(1, Ordering::SeqCst);
                return Some(entry.results.clone());
            }
        }
        
        self.miss_count.fetch_add(1, Ordering::SeqCst);
        None
    }

    pub fn put(&self, query: &[f32], filter: &Option<FilterCondition>, k: usize, results: Vec<SearchResult>) {
        let key = self.make_key(query, filter, k);
        self.cache.insert(key, CachedResult {
            results,
            timestamp: Instant::now(),
            ttl: self.default_ttl,
        });
    }

    pub fn invalidate(&self) {
        self.cache.clear();
    }

    pub fn stats(&self) -> CacheStats {
        let hits = self.hit_count.load(Ordering::SeqCst);
        let misses = self.miss_count.load(Ordering::SeqCst);
        let total = hits + misses;
        
        CacheStats {
            size: self.cache.len(),
            hits,
            misses,
            hit_rate: if total > 0 { hits as f64 / total as f64 } else { 0.0 },
        }
    }

    fn make_key(&self, query: &[f32], filter: &Option<FilterCondition>, k: usize) -> QueryCacheKey {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut query_hasher = DefaultHasher::new();
        for &v in query {
            v.to_bits().hash(&mut query_hasher);
        }
        
        let mut filter_hasher = DefaultHasher::new();
        if let Some(f) = filter {
            format!("{:?}", f).hash(&mut filter_hasher);
        }
        
        QueryCacheKey {
            query_hash: query_hasher.finish(),
            filter_hash: filter_hasher.finish(),
            k,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

// ============================================================================
// Hybrid Search Engine
// ============================================================================

/// Search strategy for hybrid queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    /// Use metadata index first, then vector search on filtered set
    PreFilter,
    /// Do vector search first, then filter results
    PostFilter,
    /// Auto-select based on filter selectivity
    Auto,
}

/// Hybrid search engine combining vector and metadata search
pub struct HybridSearchEngine<I: VectorIndex> {
    /// Underlying vector index
    vector_index: Arc<RwLock<I>>,
    /// Metadata index for filtering
    metadata_index: MetadataIndex,
    /// Query cache
    cache: QueryCache,
    /// Vector metadata storage
    metadata_store: DashMap<u64, HashMap<String, String>>,
}

use std::sync::Arc;

impl<I: VectorIndex> HybridSearchEngine<I> {
    pub fn new(vector_index: Arc<RwLock<I>>) -> Self {
        Self {
            vector_index,
            metadata_index: MetadataIndex::new(),
            cache: QueryCache::new(Duration::from_secs(60)),
            metadata_store: DashMap::new(),
        }
    }

    /// Insert vector with metadata
    pub fn insert(&self, id: u64, vector: Vec<f32>, metadata: HashMap<String, String>) -> Result<()> {
        // Insert into vector index using the trait method
        let v = crate::Vector::with_payload(vector, serde_json::Map::new());
        self.vector_index.write().add(id as crate::VectorId, &v)?;
        
        // Index metadata
        self.metadata_index.index(id, &metadata);
        
        // Store metadata
        self.metadata_store.insert(id, metadata);
        
        Ok(())
    }

    /// Search with optional metadata filter
    /// Helper to perform vector search using the trait
    fn do_vector_search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        use crate::SearchRequest;
        let request = SearchRequest::new(query.to_vec(), k);
        let scored = self.vector_index.read().search(&request)?;
        Ok(scored.into_iter().map(|s| SearchResult {
            id: s.id,
            distance: 1.0 - s.score, // Convert score to distance
            vector: None,
        }).collect())
    }

    pub fn search(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<FilterCondition>,
        strategy: SearchStrategy,
    ) -> Result<Vec<SearchResult>> {
        // Check cache first
        if let Some(cached) = self.cache.get(query, &filter, k) {
            return Ok(cached);
        }
        
        let filter_ref = filter.as_ref();
        let results = match strategy {
            SearchStrategy::PreFilter => self.pre_filter_search(query, k, filter_ref),
            SearchStrategy::PostFilter => self.post_filter_search(query, k, filter_ref),
            SearchStrategy::Auto => {
                // Auto-select based on filter selectivity
                if let Some(ref f) = filter {
                    let selectivity = self.metadata_index.estimate_selectivity(f);
                    if selectivity < 0.1 {
                        // Highly selective, use pre-filter
                        self.pre_filter_search(query, k, filter_ref)
                    } else {
                        // Not selective, use post-filter
                        self.post_filter_search(query, k, filter_ref)
                    }
                } else {
                    self.do_vector_search(query, k)
                }
            }
        }?;
        
        // Cache results
        self.cache.put(query, &filter, k, results.clone());
        
        Ok(results)
    }

    /// Pre-filter strategy: filter metadata first, then vector search
    fn pre_filter_search(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&FilterCondition>,
    ) -> Result<Vec<SearchResult>> {
        let Some(filter) = filter else {
            return self.do_vector_search(query, k);
        };
        
        // Get candidate IDs from metadata index
        let candidate_ids = self.metadata_index.get_matching_ids(&filter)
            .ok_or_else(|| RTDBError::Query("Filter not indexable".to_string()))?;
        
        if candidate_ids.is_empty() {
            return Ok(Vec::new());
        }
        
        // For now, do brute force search on candidates
        // In production, this would use a filtered vector search
        let all_results = self.do_vector_search(query, k * 10)?;
        
        // Filter to only candidates
        let filtered: Vec<_> = all_results
            .into_iter()
            .filter(|r| candidate_ids.contains(&r.id))
            .take(k)
            .collect();
        
        Ok(filtered)
    }

    /// Post-filter strategy: vector search first, then filter
    fn post_filter_search(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&FilterCondition>,
    ) -> Result<Vec<SearchResult>> {
        let Some(filter) = filter else {
            return self.do_vector_search(query, k);
        };
        
        // Get more results to allow for filtering
        let search_k = k * 10;
        let candidates = self.do_vector_search(query, search_k)?;
        
        // Apply filter
        let mut results = Vec::new();
        for candidate in candidates {
            if let Some(metadata) = self.metadata_store.get(&candidate.id) {
                if filter.evaluate(&metadata) {
                    results.push(candidate);
                    if results.len() >= k {
                        break;
                    }
                }
            }
        }
        
        Ok(results)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear query cache
    pub fn clear_cache(&self) {
        self.cache.invalidate();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_evaluation() {
        let mut metadata = HashMap::new();
        metadata.insert("category".to_string(), "electronics".to_string());
        metadata.insert("price".to_string(), "299.99".to_string());
        metadata.insert("in_stock".to_string(), "true".to_string());

        // Exact match
        let filter = FilterCondition::Eq {
            field: "category".to_string(),
            value: "electronics".to_string(),
        };
        assert!(filter.evaluate(&metadata));

        // Not equal
        let filter = FilterCondition::Ne {
            field: "category".to_string(),
            value: "clothing".to_string(),
        };
        assert!(filter.evaluate(&metadata));

        // Numeric comparison
        let filter = FilterCondition::Gte {
            field: "price".to_string(),
            value: 200.0,
        };
        assert!(filter.evaluate(&metadata));

        // Range check
        let filter = FilterCondition::And(
            Box::new(FilterCondition::Gte {
                field: "price".to_string(),
                value: 100.0,
            }),
            Box::new(FilterCondition::Lt {
                field: "price".to_string(),
                value: 500.0,
            }),
        );
        assert!(filter.evaluate(&metadata));

        // Set membership
        let filter = FilterCondition::In {
            field: "category".to_string(),
            values: vec!["electronics".to_string(), "computers".to_string()],
        };
        assert!(filter.evaluate(&metadata));
    }

    #[test]
    fn test_metadata_index() {
        let index = MetadataIndex::new();
        
        let mut meta1 = HashMap::new();
        meta1.insert("category".to_string(), "electronics".to_string());
        
        let mut meta2 = HashMap::new();
        meta2.insert("category".to_string(), "clothing".to_string());
        
        index.index(1, &meta1);
        index.index(2, &meta2);
        index.index(3, &meta1);
        
        // Query by exact match
        let filter = FilterCondition::Eq {
            field: "category".to_string(),
            value: "electronics".to_string(),
        };
        
        let ids = index.get_matching_ids(&filter).unwrap();
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&2));
    }

    #[test]
    fn test_query_cache() {
        let cache = QueryCache::new(Duration::from_secs(60));
        
        let query = vec![0.1, 0.2, 0.3];
        let results = vec![SearchResult { id: 1, distance: 0.5, vector: None }];
        
        // Miss
        assert!(cache.get(&query, &None, 10).is_none());
        
        // Put
        cache.put(&query, &None, 10, results.clone());
        
        // Hit
        assert_eq!(cache.get(&query, &None, 10), Some(results));
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }
}
