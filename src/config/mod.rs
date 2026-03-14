//! Configuration management for RTDB
//!
//! Supports YAML files, environment variables, and hot reload

use crate::{Result, RTDBError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global configuration manager with hot reload support for RTDB settings.
/// 
/// Manages application configuration with file-based loading, environment
/// variable overrides, and runtime configuration reloading capabilities.
pub struct ConfigManager {
    /// Thread-safe configuration storage
    inner: Arc<RwLock<RTDBConfig>>,
    /// Optional path to configuration file
    config_path: Option<PathBuf>,
}

impl ConfigManager {
    /// Load configuration from file and environment
    pub fn new(config_path: Option<&str>) -> Result<Self> {
        let mut builder = config::Config::builder();

        // Add default config
        builder = builder.add_source(config::File::from_str(
            include_str!("default.yaml"),
            config::FileFormat::Yaml,
        ));

        // Add user config file if provided
        if let Some(path) = config_path {
            builder = builder.add_source(config::File::new(path, config::FileFormat::Yaml));
        }

        // Add environment variables with prefix RTDB_
        builder = builder.add_source(
            config::Environment::with_prefix("RTDB")
                .separator("_")
                .prefix_separator("_"),
        );

        let settings = builder.build().map_err(|e| {
            RTDBError::Config(format!("Failed to build config: {}", e))
        })?;

        let config: RTDBConfig = settings.try_deserialize().map_err(|e| {
            RTDBError::Config(format!("Failed to deserialize config: {}", e))
        })?;

        Ok(Self {
            inner: Arc::new(RwLock::new(config)),
            config_path: config_path.map(PathBuf::from),
        })
    }

    /// Get current configuration
    pub async fn get(&self) -> RTDBConfig {
        self.inner.read().await.clone()
    }

    /// Reload configuration from file
    pub async fn reload(&self) -> Result<()> {
        let Some(ref path) = self.config_path else {
            return Err(RTDBError::Config(
                "No config file path provided".to_string()
            ));
        };

        let mut builder = config::Config::builder();

        builder = builder.add_source(config::File::from_str(
            include_str!("default.yaml"),
            config::FileFormat::Yaml,
        ));

        builder = builder.add_source(config::File::new(
            path.to_str().unwrap(),
            config::FileFormat::Yaml,
        ));

        builder = builder.add_source(
            config::Environment::with_prefix("RTDB")
                .separator("_")
                .prefix_separator("_"),
        );

        let settings = builder.build().map_err(|e| {
            RTDBError::Config(format!("Failed to rebuild config: {}", e))
        })?;

        let config: RTDBConfig = settings.try_deserialize().map_err(|e| {
            RTDBError::Config(format!("Failed to deserialize config: {}", e))
        })?;

        *self.inner.write().await = config;

        Ok(())
    }

    /// Watch config file for changes and auto-reload
    pub async fn watch(&self) -> Result<()> {
        // Stub for file watching - would use notify crate in production
        Ok(())
    }
}

/// Main RTDB configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RTDBConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Index configuration
    pub index: IndexConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Cluster configuration
    pub cluster: ClusterConfig,
    /// Security configuration
    pub security: SecurityConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// REST API bind address
    pub rest_bind: String,
    /// gRPC bind address
    pub grpc_bind: String,
    /// Metrics endpoint bind address
    pub metrics_bind: String,
    /// Maximum request size in MB
    pub max_request_size_mb: usize,
    /// Request timeout in seconds
    pub request_timeout_sec: u64,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory path
    pub data_dir: String,
    /// WAL segment size in MB
    pub wal_segment_size_mb: usize,
    /// MemTable size threshold in MB
    pub memtable_size_mb: usize,
    /// Compression type: none, lz4, zstd, snappy
    pub compression: String,
    /// Maximum SSTable size in MB
    pub max_sstable_size_mb: usize,
    /// Number of compaction threads
    pub compaction_threads: usize,
}

/// Index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    /// HNSW M parameter (connections per layer)
    pub hnsw_m: usize,
    /// HNSW ef_construction parameter
    pub hnsw_ef_construction: usize,
    /// HNSW ef_search parameter
    pub hnsw_ef_search: usize,
    /// Enable learned index
    pub enable_learned_index: bool,
    /// Number of index build threads
    pub build_threads: usize,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    pub level: String,
    /// Log format: json, pretty, compact
    pub format: String,
    /// Log output: stdout, stderr, file
    pub output: String,
    /// Log file path (if output is file)
    pub file_path: Option<String>,
}

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Node ID (0 for standalone)
    pub node_id: u64,
    /// Cluster peers (comma-separated addresses)
    pub peers: Vec<String>,
    /// Enable Raft consensus
    pub enable_raft: bool,
    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable authentication
    pub enable_auth: bool,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Enable TLS
    pub enable_tls: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS key path
    pub tls_key_path: Option<String>,
}

impl Default for RTDBConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                rest_bind: "0.0.0.0:6333".to_string(),
                grpc_bind: "0.0.0.0:6334".to_string(),
                metrics_bind: "0.0.0.0:9090".to_string(),
                max_request_size_mb: 32,
                request_timeout_sec: 30,
            },
            storage: StorageConfig {
                data_dir: "./data".to_string(),
                wal_segment_size_mb: 64,
                memtable_size_mb: 64,
                compression: "zstd".to_string(),
                max_sstable_size_mb: 256,
                compaction_threads: 2,
            },
            index: IndexConfig {
                hnsw_m: 16,
                hnsw_ef_construction: 100,
                hnsw_ef_search: 64,
                enable_learned_index: true,
                build_threads: num_cpus::get(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "pretty".to_string(),
                output: "stdout".to_string(),
                file_path: None,
            },
            cluster: ClusterConfig {
                node_id: 0,
                peers: vec![],
                enable_raft: false,
                heartbeat_interval_ms: 100,
            },
            security: SecurityConfig {
                enable_auth: false,
                api_key: None,
                enable_tls: false,
                tls_cert_path: None,
                tls_key_path: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RTDBConfig::default();
        assert_eq!(config.server.rest_bind, "0.0.0.0:6333");
        assert_eq!(config.storage.compression, "zstd");
    }
}
