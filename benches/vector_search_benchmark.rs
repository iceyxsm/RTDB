//! Vector search performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rtdb::{
    collection::CollectionManager,
    CollectionConfig, Distance, SearchRequest, UpsertRequest, Vector,
};
use std::sync::Arc;
use tempfile::TempDir;

fn generate_random_vector(dim: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dim).map(|_| rng.gen::<f32>()).collect()
}

fn benchmark_insertion(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );
    
    let config = CollectionConfig::new(128);
    manager.create_collection("bench", config).unwrap();
    let collection = manager.get_collection("bench").unwrap();
    
    let mut group = c.benchmark_group("vector_insertion");
    
    for batch_size in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &size| {
                let mut counter = 0u64;
                b.iter(|| {
                    let vectors: Vec<(u64, Vector)> = (0..size)
                        .map(|i| {
                            counter += 1;
                            (counter, Vector::new(generate_random_vector(128)))
                        })
                        .collect();
                    
                    let request = UpsertRequest { vectors };
                    black_box(collection.upsert(request).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_search(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let manager = Arc::new(
        CollectionManager::new(temp_dir.path()).unwrap()
    );
    
    let config = CollectionConfig::new(128);
    manager.create_collection("search_bench", config).unwrap();
    let collection = manager.get_collection("search_bench").unwrap();
    
    // Insert test data
    let vectors: Vec<(u64, Vector)> = (0..1000)
        .map(|i| (i, Vector::new(generate_random_vector(128))))
        .collect();
    
    collection.upsert(UpsertRequest { vectors }).unwrap();
    
    let mut group = c.benchmark_group("vector_search");
    
    for k in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(k),
            k,
            |b, &k| {
                let query = generate_random_vector(128);
                b.iter(|| {
                    let request = SearchRequest::new(query.clone(), k);
                    black_box(collection.search(request).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_search_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_scaling");
    
    for &size in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                // Setup for each benchmark
                let temp_dir = TempDir::new().unwrap();
                let manager = Arc::new(
                    CollectionManager::new(temp_dir.path()).unwrap()
                );
                
                let config = CollectionConfig::new(128);
                manager.create_collection("scale", config).unwrap();
                let collection = manager.get_collection("scale").unwrap();
                
                // Insert data
                let vectors: Vec<(u64, Vector)> = (0..size)
                    .map(|i| (i, Vector::new(generate_random_vector(128))))
                    .collect();
                collection.upsert(UpsertRequest { vectors }).unwrap();
                
                let query = generate_random_vector(128);
                
                b.iter(|| {
                    let request = SearchRequest::new(query.clone(), 10);
                    black_box(collection.search(request).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_different_dimensions(c: &mut Criterion) {
    let mut group = c.benchmark_group("dimension_scaling");
    
    for &dim in &[64, 128, 256, 512, 768] {
        group.bench_with_input(
            BenchmarkId::from_parameter(dim),
            &dim,
            |b, &dim| {
                let temp_dir = TempDir::new().unwrap();
                let manager = Arc::new(
                    CollectionManager::new(temp_dir.path()).unwrap()
                );
                
                let config = CollectionConfig::new(dim);
                manager.create_collection("dim", config).unwrap();
                let collection = manager.get_collection("dim").unwrap();
                
                // Insert data
                let vectors: Vec<(u64, Vector)> = (0..1000)
                    .map(|i| (i, Vector::new(generate_random_vector(dim))))
                    .collect();
                collection.upsert(UpsertRequest { vectors }).unwrap();
                
                let query = generate_random_vector(dim);
                
                b.iter(|| {
                    let request = SearchRequest::new(query.clone(), 10);
                    black_box(collection.search(request).unwrap());
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_insertion,
    benchmark_search,
    benchmark_search_scale,
    benchmark_different_dimensions
);
criterion_main!(benches);
