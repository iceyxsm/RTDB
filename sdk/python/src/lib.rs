//! Python SDK for RTDB Vector Database
//!
//! This module provides Python bindings for the RTDB vector database
//! using PyO3 for high-performance FFI.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};
use pyo3_asyncio::tokio::future_into_py;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Python wrapper for RTDB Client
#[pyclass]
struct RtdbClient {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

#[pymethods]
impl RtdbClient {
    /// Create a new RTDB client
    ///
    /// Args:
    ///     url: Base URL of the RTDB server (e.g., "http://localhost:6333")
    ///     api_key: Optional API key for authentication
    #[new]
    #[pyo3(signature = (url, api_key=None))]
    fn new(url: String, api_key: Option<String>) -> PyResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create client: {}", e)))?;

        Ok(Self {
            base_url: url.trim_end_matches('/').to_string(),
            api_key,
            client,
        })
    }

    /// Check if server is healthy
    fn is_healthy<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/healthz", self.base_url);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let mut request = client.get(&url);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => Ok(response.status().is_success()),
                Err(_) => Ok(false),
            }
        })
    }

    /// Create a new collection
    #[pyo3(signature = (name, dimension, distance="Cosine"))]
    fn create_collection<'py>(
        &self,
        py: Python<'py>,
        name: String,
        dimension: usize,
        distance: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}", self.base_url, name);
        let api_key = self.api_key.clone();
        let distance = distance.to_string();

        future_into_py(py, async move {
            let body = serde_json::json!({
                "dimension": dimension,
                "distance": distance
            });

            let mut request = client.put(&url).json(&body);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        Ok(true)
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to create collection: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// Delete a collection
    fn delete_collection<'py>(&self, py: Python<'py>, name: String) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}", self.base_url, name);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let mut request = client.delete(&url);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        Ok(true)
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to delete collection: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// List all collections
    fn list_collections<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections", self.base_url);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let mut request = client.get(&url);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let body: serde_json::Value = response
                            .json()
                            .await
                            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse response: {}", e)))?;
                        
                        let collections: Vec<String> = body["result"]["collections"]
                            .as_array()
                            .unwrap_or(&vec![])
                            .iter()
                            .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
                            .collect();
                        
                        Python::with_gil(|py| {
                            let list = PyList::empty_bound(py);
                            for name in collections {
                                list.append(PyString::new_bound(py, &name)).unwrap();
                            }
                            Ok::<_, PyErr>(list.into())
                        })
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to list collections: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// Upsert points into a collection
    fn upsert<'py>(
        &self,
        py: Python<'py>,
        collection_name: String,
        points: &Bound<'py, PyList>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}/points", self.base_url, collection_name);
        let api_key = self.api_key.clone();

        // Convert Python points to JSON
        let points_json: Vec<serde_json::Value> = Python::with_gil(|py| {
            points.iter().map(|point| {
                let dict = point.downcast::<PyDict>()?;
                let id: u64 = dict.get_item("id")?.unwrap().extract()?;
                let vector: Vec<f32> = dict.get_item("vector")?.unwrap().extract()?;
                
                let mut json_point = serde_json::json!({
                    "id": id,
                    "vector": vector
                });

                if let Ok(Some(payload)) = dict.get_item("payload") {
                    let payload_json: HashMap<String, serde_json::Value> = payload.extract()?;
                    json_point["payload"] = serde_json::json!(payload_json);
                }

                Ok::<_, PyErr>(json_point)
            }).collect::<Result<Vec<_>, _>>()
        }).map_err(|e: PyErr| pyo3::exceptions::PyValueError::new_err(format!("Invalid points format: {}", e)))?;

        future_into_py(py, async move {
            let body = serde_json::json!({ "points": points_json });

            let mut request = client.put(&url).json(&body);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let body: serde_json::Value = response
                            .json()
                            .await
                            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse response: {}", e)))?;
                        Ok(body["result"]["status"].as_str().unwrap_or("unknown").to_string())
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to upsert points: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// Search for similar vectors
    #[pyo3(signature = (collection_name, vector, limit=10, with_payload=true))]
    fn search<'py>(
        &self,
        py: Python<'py>,
        collection_name: String,
        vector: Vec<f32>,
        limit: usize,
        with_payload: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}/points/search", self.base_url, collection_name);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let body = serde_json::json!({
                "vector": vector,
                "limit": limit,
                "with_payload": with_payload
            });

            let mut request = client.post(&url).json(&body);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let body: serde_json::Value = response
                            .json()
                            .await
                            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse response: {}", e)))?;
                        
                        Python::with_gil(|py| {
                            let results = PyList::empty_bound(py);
                            if let Some(points) = body["result"].as_array() {
                                for point in points {
                                    let dict = PyDict::new_bound(py);
                                    
                                    if let Some(id) = point["id"].as_u64() {
                                        dict.set_item("id", id).unwrap();
                                    }
                                    
                                    if let Some(score) = point["score"].as_f64() {
                                        dict.set_item("score", score).unwrap();
                                    }
                                    
                                    if let Some(payload) = point["payload"].as_object() {
                                        let payload_dict = PyDict::new_bound(py);
                                        for (key, value) in payload {
                                            let value_str = value.to_string();
                                            payload_dict.set_item(key, value_str).unwrap();
                                        }
                                        dict.set_item("payload", payload_dict).unwrap();
                                    }
                                    
                                    results.append(dict).unwrap();
                                }
                            }
                            Ok::<_, PyErr>(results.into())
                        })
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to search: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// Get a point by ID
    fn get_point<'py>(
        &self,
        py: Python<'py>,
        collection_name: String,
        point_id: u64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}/points/{}", self.base_url, collection_name, point_id);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let mut request = client.get(&url);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let body: serde_json::Value = response
                            .json()
                            .await
                            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to parse response: {}", e)))?;
                        
                        Python::with_gil(|py| {
                            let dict = PyDict::new_bound(py);
                            
                            if let Some(result) = body["result"].as_object() {
                                if let Some(id) = result.get("id").and_then(|v| v.as_u64()) {
                                    dict.set_item("id", id).unwrap();
                                }
                                
                                if let Some(vector) = result.get("vector").and_then(|v| v.as_array()) {
                                    let vec_list = PyList::empty_bound(py);
                                    for val in vector {
                                        if let Some(f) = val.as_f64() {
                                            vec_list.append(f).unwrap();
                                        }
                                    }
                                    dict.set_item("vector", vec_list).unwrap();
                                }
                                
                                if let Some(payload) = result.get("payload").and_then(|v| v.as_object()) {
                                    let payload_dict = PyDict::new_bound(py);
                                    for (key, value) in payload {
                                        let value_str = value.to_string();
                                        payload_dict.set_item(key, value_str).unwrap();
                                    }
                                    dict.set_item("payload", payload_dict).unwrap();
                                }
                            }
                            
                            Ok::<_, PyErr>(dict.into())
                        })
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to get point: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }

    /// Delete a point by ID
    fn delete_point<'py>(
        &self,
        py: Python<'py>,
        collection_name: String,
        point_id: u64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.client.clone();
        let url = format!("{}/collections/{}/points/{}", self.base_url, collection_name, point_id);
        let api_key = self.api_key.clone();

        future_into_py(py, async move {
            let mut request = client.delete(&url);
            if let Some(key) = api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        Ok(true)
                    } else {
                        let error_text = response.text().await.unwrap_or_default();
                        Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to delete point: {}",
                            error_text
                        )))
                    }
                }
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Request failed: {}",
                    e
                ))),
            }
        })
    }
}

/// Point struct for Python
#[pyclass]
#[derive(Clone, Debug)]
struct Point {
    #[pyo3(get, set)]
    id: u64,
    #[pyo3(get, set)]
    vector: Vec<f32>,
    #[pyo3(get, set)]
    payload: Option<HashMap<String, String>>,
}

#[pymethods]
impl Point {
    #[new]
    fn new(id: u64, vector: Vec<f32>, payload: Option<HashMap<String, String>>) -> Self {
        Self {
            id,
            vector,
            payload,
        }
    }

    fn __repr__(&self) -> String {
        format!("Point(id={}, vector_len={}, payload={:?})", self.id, self.vector.len(), self.payload)
    }
}

/// Search result for Python
#[pyclass]
#[derive(Clone, Debug)]
struct SearchResult {
    #[pyo3(get, set)]
    id: u64,
    #[pyo3(get, set)]
    score: f32,
    #[pyo3(get, set)]
    payload: Option<HashMap<String, String>>,
}

#[pymethods]
impl SearchResult {
    #[new]
    fn new(id: u64, score: f32, payload: Option<HashMap<String, String>>) -> Self {
        Self {
            id,
            score,
            payload,
        }
    }

    fn __repr__(&self) -> String {
        format!("SearchResult(id={}, score={})", self.id, self.score)
    }
}

/// Python module definition
#[pymodule]
fn rtdb_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RtdbClient>()?;
    m.add_class::<Point>()?;
    m.add_class::<SearchResult>()?;
    
    // Add module docstring
    m.setattr("__doc__", "RTDB Python SDK - High-performance vector database client")?;
    
    Ok(())
}
