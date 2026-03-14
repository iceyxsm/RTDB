# RTDB Migration Tools

## Overview

RTDB provides production-grade migration tools to seamlessly migrate data from other vector databases and file formats. The migration system supports multiple source types, migration strategies, and provides comprehensive monitoring and validation capabilities.

## Supported Sources

### Vector Databases
- **Qdrant**: Full REST API support with authentication
- **Milvus**: REST API v1/v2 compatibility with PyMilvus client support
- **Weaviate**: GraphQL and REST API support with hybrid search
- **Pinecone**: REST API with proper authentication handling
- **LanceDB**: File-based parquet reading with JSONL fallback

### File Formats
- **JSONL**: JSON Lines format with flexible field mapping
- **CSV**: Comma-separated values with vector encoding support
- **Binary**: Custom efficient binary format for high-performance transfers
- **Parquet**: Apache Parquet format (requires arrow-rs dependency)
- **HDF5**: Hierarchical Data Format (requires hdf5 dependency)

## Migration Strategies

### 1. Streaming Migration (Default)
- **Use Case**: Standard migration for most scenarios
- **Process**: Batch-based streaming with checkpoints
- **Benefits**: Memory efficient, resumable, progress tracking
- **Downtime**: Minimal (read-only on source)

### 2. Dual-Write Migration
- **Use Case**: Zero-downtime migrations for production systems
- **Process**: Write to both systems -> Backfill -> Verify -> Switch
- **Benefits**: No downtime, consistency verification
- **Complexity**: Higher operational complexity

### 3. Blue-Green Migration
- **Use Case**: Complete environment switches
- **Process**: Prepare green -> Migrate -> Warm up -> Switch -> Verify
- **Benefits**: Full rollback capability, isolated environments
- **Resources**: Requires duplicate infrastructure

### 4. Snapshot Migration
- **Use Case**: Point-in-time consistent migrations
- **Process**: Create snapshot -> Transfer -> Apply incremental -> Verify
- **Benefits**: Consistency guarantees, incremental updates
- **Requirements**: Source system snapshot support

## Key Features

### Data Transformation
- **Field Renaming**: Map field names between systems
- **Type Conversion**: Convert between data types (string<->number, array<->string)
- **Value Mapping**: Map categorical values using lookup tables
- **Filtering**: Apply conditions to include/exclude records

### Validation & Quality Assurance
- **Vector Validation**: Dimension checking, NaN/Inf detection
- **Metadata Validation**: Required fields, type checking
- **Duplicate Detection**: ID-based and content-based deduplication
- **Schema Validation**: Ensure target compatibility

### Progress Tracking & Monitoring
- **Real-time Progress**: Records processed, failed, throughput
- **ETA Calculation**: Estimated completion time
- **Checkpoint System**: Resume interrupted migrations
- **Error Reporting**: Detailed error messages and statistics

### Authentication & Security
- **API Key Authentication**: Support for various API key formats
- **Bearer Token**: OAuth2/JWT token support
- **Custom Headers**: Flexible header-based authentication
- **Username/Password**: Basic authentication support

## Usage Examples

### CLI Migration
```bash
# Migrate from Qdrant to RTDB
rtdb migrate qdrant \
  --source-url http://localhost:6333 \
  --target-url http://localhost:6334 \
  --source-collection my_collection \
  --target-collection migrated_collection \
  --batch-size 1000 \
  --strategy streaming

# Migrate from JSONL file
rtdb migrate jsonl \
  --source-url ./vectors.jsonl \
  --target-url http://localhost:6334 \
  --target-collection imported_vectors \
  --dry-run

# Resume interrupted migration
rtdb migrate resume --migration-id <uuid>
```

### Programmatic Usage
```rust
use rtdb::migration::{MigrationConfig, MigrationManager, SourceType, MigrationStrategy};

// Create migration configuration
let config = MigrationConfig {
    source_type: SourceType::Qdrant,
    source_url: "http://localhost:6333".to_string(),
    target_url: "http://localhost:6334".to_string(),
    source_collection: Some("my_collection".to_string()),
    target_collection: "migrated_collection".to_string(),
    batch_size: 1000,
    max_concurrency: 4,
    strategy: MigrationStrategy::Stream,
    dry_run: false,
    resume: false,
    // ... other configuration
};

// Start migration
let manager = MigrationManager::new(checkpoint_dir)?;
let migration_id = manager.start_migration(config).await?;

// Monitor progress
let progress = manager.get_progress(migration_id).await;
println!("Progress: {:.1}%", progress.completion_percentage);
```

## Configuration Options

### Migration Config
```yaml
source:
  type: qdrant  # qdrant, milvus, weaviate, pinecone, lancedb, jsonl, parquet, hdf5, csv
  url: "http://localhost:6333"
  collection: "my_collection"
  auth:
    api_key: "your-api-key"
    headers:
      "Custom-Header": "value"

target:
  url: "http://localhost:6334"
  collection: "migrated_collection"
  auth:
    api_key: "target-api-key"

migration:
  strategy: streaming  # streaming, dual-write, blue-green, snapshot
  batch_size: 1000
  max_concurrency: 4
  dry_run: false
  resume: false

transformations:
  - field: "old_field"
    operation:
      rename: "new_field"
  - field: "category"
    operation:
      map:
        "tech": "technology"
        "sci": "science"

validation:
  validate_vectors: true
  validate_metadata: true
  check_duplicates: false
  vector_dimension: 768
  required_fields: ["title", "content"]
```

## Performance Considerations

### Throughput Optimization
- **Batch Size**: Larger batches (1000-5000) for better throughput
- **Concurrency**: 2-8 concurrent batches depending on system resources
- **Network**: Use same region/zone for source and target
- **Memory**: Monitor memory usage with large batches

### Source System Impact
- **Rate Limiting**: Respect source system rate limits
- **Read Replicas**: Use read replicas when available
- **Off-Peak Hours**: Schedule migrations during low-traffic periods
- **Connection Pooling**: Reuse connections for better performance

### Target System Optimization
- **Index Building**: Disable indexing during bulk import if possible
- **Batch Commits**: Use batch commits for better write performance
- **Resource Scaling**: Scale target system for migration workload
- **Monitoring**: Monitor target system metrics during migration

## Error Handling & Recovery

### Common Issues
1. **Network Timeouts**: Increase timeout values, reduce batch size
2. **Memory Issues**: Reduce batch size and concurrency
3. **Authentication Failures**: Verify credentials and permissions
4. **Schema Mismatches**: Use transformations to map fields
5. **Rate Limiting**: Implement backoff and retry logic

### Recovery Strategies
- **Checkpoints**: Automatic checkpoint saving every N batches
- **Resume**: Resume from last successful checkpoint
- **Retry Logic**: Exponential backoff for transient failures
- **Error Logging**: Detailed error logs for debugging
- **Validation**: Pre-migration validation to catch issues early

## Monitoring & Observability

### Metrics
- **Records/Second**: Current and average throughput
- **Success Rate**: Percentage of successfully migrated records
- **Error Rate**: Failed records and error types
- **Progress**: Completion percentage and ETA
- **Resource Usage**: Memory, CPU, and network utilization

### Logging
- **Structured Logs**: JSON format with correlation IDs
- **Error Details**: Full error context and stack traces
- **Progress Updates**: Regular progress checkpoints
- **Performance Stats**: Batch timing and throughput metrics

### Alerting
- **Migration Failures**: Alert on migration failures
- **Performance Degradation**: Alert on throughput drops
- **Error Spikes**: Alert on high error rates
- **Resource Exhaustion**: Alert on resource limits

## Best Practices

### Pre-Migration
1. **Backup**: Always backup source data before migration
2. **Test Migration**: Run test migrations with sample data
3. **Capacity Planning**: Ensure target system can handle the load
4. **Schema Mapping**: Plan field mappings and transformations
5. **Validation Rules**: Define validation criteria upfront

### During Migration
1. **Monitor Progress**: Actively monitor migration progress
2. **Resource Monitoring**: Watch system resources on both sides
3. **Error Handling**: Have procedures for handling errors
4. **Communication**: Keep stakeholders informed of progress
5. **Rollback Plan**: Have rollback procedures ready

### Post-Migration
1. **Data Validation**: Verify data integrity and completeness
2. **Performance Testing**: Test query performance on migrated data
3. **Index Optimization**: Rebuild/optimize indexes if needed
4. **Cleanup**: Clean up temporary files and checkpoints
5. **Documentation**: Document any issues and resolutions

## Troubleshooting

### Common Error Messages

#### "Failed to connect to source database"
- **Cause**: Network connectivity or authentication issues
- **Solution**: Verify URL, credentials, and network connectivity

#### "Vector dimension mismatch"
- **Cause**: Source vectors don't match expected dimensions
- **Solution**: Check vector dimensions, update validation config

#### "Required field missing"
- **Cause**: Source records missing required metadata fields
- **Solution**: Update required_fields config or add transformations

#### "Checkpoint corruption"
- **Cause**: Checkpoint file corrupted or incomplete
- **Solution**: Delete checkpoint and restart migration

#### "Target collection already exists"
- **Cause**: Target collection exists and migration not configured to overwrite
- **Solution**: Use different collection name or enable overwrite

### Performance Issues

#### "Migration too slow"
- **Solutions**: Increase batch size, add concurrency, use faster network
- **Check**: Source system performance, network latency, target system load

#### "High memory usage"
- **Solutions**: Reduce batch size, reduce concurrency, enable streaming
- **Check**: Vector sizes, metadata sizes, system memory limits

#### "Connection timeouts"
- **Solutions**: Increase timeout values, reduce batch size, check network
- **Check**: Network stability, system load, connection limits

## API Reference

### Migration Manager
```rust
impl MigrationManager {
    pub fn new(checkpoint_dir: PathBuf) -> Result<Self>;
    pub async fn start_migration(&self, config: MigrationConfig) -> Result<Uuid>;
    pub async fn get_progress(&self, migration_id: Uuid) -> Option<MigrationProgress>;
    pub async fn list_migrations(&self) -> Vec<MigrationProgress>;
    pub async fn cancel_migration(&self, migration_id: Uuid) -> Result<()>;
}
```

### Format Conversion
```rust
impl FormatConverter {
    pub async fn convert(
        input_path: &Path,
        output_path: &Path,
        input_format: Option<DataFormat>,
        output_format: Option<DataFormat>,
        batch_size: usize,
    ) -> Result<u64>;
}
```

### Validation
```rust
pub fn validate_record(record: &VectorRecord, config: &ValidationConfig) -> Result<()>;
pub fn validate_batch(records: &[VectorRecord], config: &ValidationConfig) -> ValidationResult;
```

## Future Enhancements

### Planned Features
- **GPU Acceleration**: CUDA-based vector processing for large migrations
- **Distributed Migration**: Multi-node migration for massive datasets
- **Schema Evolution**: Automatic schema migration and versioning
- **Real-time Sync**: Continuous synchronization between systems
- **Advanced Transformations**: ML-based data transformations

### Integration Roadmap
- **Kubernetes Operator**: Native Kubernetes migration jobs
- **Terraform Provider**: Infrastructure-as-code migration resources
- **Monitoring Integration**: Prometheus, Grafana, DataDog integration
- **CI/CD Integration**: GitHub Actions, GitLab CI migration workflows
- **Cloud Storage**: Direct S3, GCS, Azure Blob integration

## Support

For migration-related issues:
1. Check the troubleshooting section above
2. Review migration logs for detailed error information
3. Consult the API documentation for configuration options
4. Open an issue with migration configuration and error logs

## Contributing

To contribute to the migration tools:
1. Add tests for new source types or formats
2. Follow the existing client interface patterns
3. Include comprehensive error handling
4. Add documentation and examples
5. Ensure backward compatibility