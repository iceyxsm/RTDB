//! HNSW (Hierarchical Navigable Small World) implementation
//! 
//! Graph-based approximate nearest neighbor search

use crate::{Distance, HnswConfig, Result, ScoredVector, SearchRequest, Vector, VectorId};
use ordered_float::OrderedFloat;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// HNSW index
pub struct HNSWIndex {
    config: HnswConfig,
    distance: Distance,
    layers: Vec<Layer>,
    max_layer: usize,
    entry_point: Option<VectorId>,
    vectors: HashMap<VectorId, Vector>,
}

struct Layer {
    /// Graph edges
    edges: HashMap<VectorId, Vec<VectorId>>,
}

impl HNSWIndex {
    /// Create new HNSW index
    pub fn new(config: HnswConfig, distance: Distance) -> Self {
        Self {
            config,
            distance,
            layers: Vec::new(),
            max_layer: 0,
            entry_point: None,
            vectors: HashMap::new(),
        }
    }

    /// Get random level for new node
    fn random_level(&self) -> usize {
        // Simple random level selection
        // P(layer = k) = exp(-k / m)
        let m = self.config.m as f64;
        let mut level = 0;
        let mut r: f64 = rand::random::<f64>();
        
        while r < (-1.0_f64 / m).exp() && level < 16 {
            level += 1;
            r = rand::random::<f64>();
        }
        
        level
    }

    /// Search layer
    fn search_layer(
        &self,
        query: &Vector,
        entry: VectorId,
        ef: usize,
        layer_idx: usize,
    ) -> Vec<ScoredVector> {
        let mut visited = HashSet::new();
        let mut candidates: BinaryHeap<std::cmp::Reverse<(OrderedFloat<f32>, VectorId)>> = BinaryHeap::new();
        let mut results: BinaryHeap<(OrderedFloat<f32>, VectorId)> = BinaryHeap::new();

        if let Some(dist) = self.distance(&query.data, &entry) {
            candidates.push(std::cmp::Reverse((OrderedFloat(dist), entry)));
            results.push((OrderedFloat(dist), entry));
            visited.insert(entry);
        }

        while let Some(std::cmp::Reverse((dist, current))) = candidates.pop() {
            if let Some((worst_dist, _)) = results.peek() {
                if dist > *worst_dist && results.len() >= ef {
                    break;
                }
            }
            let current_f32 = dist.0;

            if let Some(layer) = self.layers.get(layer_idx) {
                if let Some(neighbors) = layer.edges.get(&current) {
                    for &neighbor in neighbors {
                        if visited.insert(neighbor) {
                            if let Some(n_dist) = self.distance(&query.data, &neighbor) {
                                if results.len() < ef {
                                    candidates.push(std::cmp::Reverse((OrderedFloat(n_dist), neighbor)));
                                    results.push((OrderedFloat(n_dist), neighbor));
                                } else if let Some((worst, _)) = results.peek() {
                                    if OrderedFloat(n_dist) < *worst {
                                        candidates.push(std::cmp::Reverse((OrderedFloat(n_dist), neighbor)));
                                        results.pop();
                                        results.push((OrderedFloat(n_dist), neighbor));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results
            .into_sorted_vec()
            .into_iter()
            .map(|(score, id)| ScoredVector {
                id,
                score: score.0,
                vector: None,
                payload: None,
            })
            .collect()
    }

    /// Compute distance
    fn distance(&self, a: &[f32], b_id: &VectorId) -> Option<f32> {
        self.vectors.get(b_id).and_then(|b| {
            self.distance.calculate(a, &b.data).ok()
        })
    }

    /// Select neighbors using heuristic
    fn select_neighbors(
        &self,
        candidates: &[ScoredVector],
        m: usize,
        _layer: usize,
    ) -> Vec<VectorId> {
        // Simple: take top M by distance
        candidates.iter().take(m).map(|s| s.id).collect()
    }
}

impl super::VectorIndex for HNSWIndex {
    fn add(&mut self, id: VectorId, vector: &Vector) -> Result<()> {
        // Ensure enough layers
        let level = self.random_level();
        while self.layers.len() <= level {
            self.layers.push(Layer { edges: HashMap::new() });
        }

        // Insert vector
        self.vectors.insert(id, vector.clone());

        // Build connections
        if let Some(entry) = self.entry_point {
            let mut current_entry = entry;

            // Search from top layer down
            for layer_idx in (level + 1..self.layers.len()).rev() {
                let nearest = self.search_layer(vector, current_entry, 1, layer_idx);
                if let Some(first) = nearest.first() {
                    current_entry = first.id;
                }
            }

            // Add connections at each level
            for layer_idx in (0..=level.min(self.layers.len() - 1)).rev() {
                let ef = if layer_idx == 0 {
                    self.config.ef_construct
                } else {
                    self.config.m
                };

                let nearest = self.search_layer(vector, current_entry, ef, layer_idx);
                let neighbors = self.select_neighbors(&nearest, self.config.m, layer_idx);

                if let Some(layer) = self.layers.get_mut(layer_idx) {
                    layer.edges.insert(id, neighbors.clone());

                    // Add reverse edges
                    for neighbor in neighbors {
                        if let Some(edges) = layer.edges.get_mut(&neighbor) {
                            if !edges.contains(&id) {
                                edges.push(id);
                            }
                        }
                    }
                }

                if let Some(first) = nearest.first() {
                    current_entry = first.id;
                }
            }
        } else {
            self.entry_point = Some(id);
        }

        if level > self.max_layer {
            self.max_layer = level;
            self.entry_point = Some(id);
        }

        Ok(())
    }

    fn remove(&mut self, id: VectorId) -> Result<()> {
        // Remove from all layers
        for layer in &mut self.layers {
            layer.edges.remove(&id);
            for edges in layer.edges.values_mut() {
                edges.retain(|&e| e != id);
            }
        }

        self.vectors.remove(&id);
        Ok(())
    }

    fn search(&self, request: &SearchRequest) -> Result<Vec<ScoredVector>> {
        let query = Vector::new(request.vector.clone());
        
        if let Some(entry) = self.entry_point {
            let ef = request.params.as_ref()
                .and_then(|p| p.hnsw_ef)
                .unwrap_or(self.config.ef);

            let mut current_entry = entry;

            // Search from top layer
            for layer_idx in (1..self.layers.len()).rev() {
                let nearest = self.search_layer(&query, current_entry, 1, layer_idx);
                if let Some(first) = nearest.first() {
                    current_entry = first.id;
                }
            }

            // Search bottom layer with full ef
            let results = self.search_layer(&query, current_entry, ef.max(request.limit), 0);
            Ok(results.into_iter().take(request.limit).collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn len(&self) -> usize {
        self.vectors.len()
    }

    fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    fn build(&mut self, vectors: &[(VectorId, Vector)]) -> Result<()> {
        for (id, vector) in vectors {
            self.add(*id, vector)?;
        }
        Ok(())
    }
}
