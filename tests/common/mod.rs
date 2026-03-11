//! Common test utilities for integration tests

use reqwest::Client;
use std::sync::OnceLock;

/// Shared HTTP client
static CLIENT: OnceLock<Client> = OnceLock::new();

fn get_client() -> &'static Client {
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client")
    })
}

/// Test application wrapper
pub struct TestApp {
    #[allow(dead_code)]
    temp_dir: tempfile::TempDir,
}

impl TestApp {
    /// Create a new test application instance
    pub async fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        Self { temp_dir }
    }

    /// Get the HTTP client
    pub fn client(&self) -> &'static Client {
        get_client()
    }

    /// Create a collection
    pub async fn create_collection(&self, name: &str, dimension: usize, distance: &str) {
        let client = get_client();
        let response = client
            .put(format!("http://localhost:6333/collections/{}", name))
            .json(&serde_json::json!({
                "dimension": dimension,
                "distance": distance
            }))
            .send()
            .await
            .expect("Request failed");

        assert!(
            response.status().is_success(),
            "Failed to create collection: {:?}",
            response.text().await
        );
    }

    /// Upsert points into a collection
    pub async fn upsert_points(
        &self,
        collection: &str,
        points: Vec<(u64, Vec<f32>, Option<serde_json::Value>)>,
    ) {
        let client = get_client();
        let points_json: Vec<_> = points
            .into_iter()
            .map(|(id, vector, payload)| {
                let mut point = serde_json::json!({
                    "id": id,
                    "vector": vector
                });
                if let Some(p) = payload {
                    point["payload"] = p;
                }
                point
            })
            .collect();

        let response = client
            .put(format!(
                "http://localhost:6333/collections/{}/points",
                collection
            ))
            .json(&serde_json::json!({ "points": points_json }))
            .send()
            .await
            .expect("Request failed");

        assert!(
            response.status().is_success(),
            "Failed to upsert points: {:?}",
            response.text().await
        );
    }

    /// Upsert a large batch of points
    pub async fn upsert_points_batch(
        &self,
        collection: &str,
        points: Vec<(u64, Vec<f32>, Option<serde_json::Value>)>,
    ) {
        // Process in batches of 100
        for chunk in points.chunks(100) {
            let points_json: Vec<_> = chunk
                .iter()
                .cloned()
                .map(|(id, vector, payload)| {
                    let mut point = serde_json::json!({
                        "id": id,
                        "vector": vector
                    });
                    if let Some(p) = payload {
                        point["payload"] = p;
                    }
                    point
                })
                .collect();

            let client = get_client();
            let response = client
                .put(format!(
                    "http://localhost:6333/collections/{}/points",
                    collection
                ))
                .json(&serde_json::json!({ "points": points_json }))
                .send()
                .await
                .expect("Request failed");

            assert!(
                response.status().is_success(),
                "Failed to upsert points batch: {:?}",
                response.text().await
            );
        }
    }

    /// Delete a collection
    pub async fn delete_collection(&self, name: &str) {
        let client = get_client();
        let response = client
            .delete(format!("http://localhost:6333/collections/{}", name))
            .send()
            .await
            .expect("Request failed");

        assert!(
            response.status().is_success(),
            "Failed to delete collection: {:?}",
            response.text().await
        );
    }

    /// Make a GET request
    pub async fn get(&self, path: &str) -> reqwest::Response {
        let client = get_client();
        client
            .get(format!("http://localhost:6333{}", path))
            .send()
            .await
            .expect("Request failed")
    }

    /// Make a POST request with JSON body
    pub async fn post(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        let client = get_client();
        client
            .post(format!("http://localhost:6333{}", path))
            .json(&body)
            .send()
            .await
            .expect("Request failed")
    }

    /// Make a PUT request with JSON body
    pub async fn put(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        let client = get_client();
        client
            .put(format!("http://localhost:6333{}", path))
            .json(&body)
            .send()
            .await
            .expect("Request failed")
    }

    /// Make a DELETE request
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        let client = get_client();
        client
            .delete(format!("http://localhost:6333{}", path))
            .send()
            .await
            .expect("Request failed")
    }
}

/// Test data generators
pub mod generators {
    use rand::Rng;

    /// Generate a random vector of given dimension
    pub fn random_vector(dim: usize) -> Vec<f32> {
        let mut rng = rand::thread_rng();
        (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
    }
}
