# dl-driver AI Coding Instructions

This is a **high-performance AI/ML data loading framework** built in Rust that provides unified access to multiple storage backends (File, S3, Azure, DirectIO) through the s3dlio library. The project is designed as a drop-in replacement for DLIO benchmarks with enterprise-grade performance and **full MLCommons DLIO compatibility**.

## Available Development Tools

When working with this codebase, the following search and analysis tools are available:
- **ripgrep (`rg`)**: Fast text search across files with regex support
- **fd**: Fast alternative to `find` for locating files
- **Standard Rust toolchain**: cargo, rustc, clippy, rustfmt

Use these tools for efficient code exploration and debugging.

## 🚨 CRITICAL: Large Data Directory Guidelines

**NEVER use `/tmp/` for test data larger than 1 GB!**

When creating test datasets or running data generation commands that will produce more than 1 GB of data, **ALWAYS** use one of these high-capacity directories:
- `/mnt/vast1/` - Primary large data directory
- `/mnt/vast2/` - Secondary large data directory  
- `/mnt/vast3/` - Tertiary large data directory

**Examples:**
- ❌ **WRONG**: `data_folder: file:///tmp/dlio_data` (for large datasets)
- ✅ **CORRECT**: `data_folder: file:///mnt/vast1/dlio_data`
- ❌ **WRONG**: Test directory `/tmp/format_validation` (if generating >1GB)
- ✅ **CORRECT**: Test directory `/mnt/vast1/dl_driver_format_validation`

**Update all test configs and validation scripts to use `/mnt/vast1`, `/mnt/vast2`, or `/mnt/vast3` when appropriate.**

## Architecture Overview

**Workspace Structure**: 5-crate workspace architecture
- `crates/core/` - Configuration parsing, workload orchestration, metrics
- `crates/storage/` - Storage backend abstractions (POSIX-focused)
- `crates/formats/` - Data format handlers (NPZ, HDF5, etc.)
- `crates/cli/` - Command-line interface and main binary
- `crates/py_api/` - Python bindings via PyO3

**Key Pattern**: All storage operations use **s3dlio's ObjectStore** trait, not the local storage crate. The `storage/` crate is legacy/fallback.

## Core Abstractions

### Configuration System
- **MLCommons DLIO compatibility**: Full support for DLIO benchmark YAML configurations via `DlioConfig::from_yaml_file()`
- **Legacy dl-driver configs**: Backward compatibility via `Config::from_yaml_file()`
- **Dual CLI interface**: `dl-driver legacy` (old format) and `dl-driver dlio` (MLCommons format)
- **URI-based backend detection**: Auto-detects storage backend from `data_folder` URI scheme
- **YAML↔JSON conversion**: Seamless translation between DLIO YAML and s3dlio JSON formats

```rust
// Pattern: Always check storage backend type via config
match config.storage_backend() {
    StorageBackend::S3 => // Use S3 credentials
    StorageBackend::Azure => // Use Azure environment vars
    StorageBackend::DirectIO => // Use O_DIRECT optimizations
    StorageBackend::File => // Use standard filesystem
}
```

### DLIO Integration Layer
- **Complete DLIO schema support**: All MLCommons DLIO configuration fields in `core/src/dlio_compat.rs`
- **Automatic conversion**: DLIO configs → s3dlio LoaderOptions and PoolConfig
- **Comprehensive validation**: 9 test suites validate parsing of real MLCommons benchmark configs
- **Backend URI mapping**: `data_folder` URIs automatically map to appropriate s3dlio backends

### Workload Execution Engine
- **Primary class**: `WorkloadRunner` in `core/src/workload.rs`
- **Async execution**: All I/O operations are async using s3dlio's `AsyncPoolDataLoader`
- **Three-phase workflow**: Data generation → Training → Checkpointing
- **Performance metrics**: Comprehensive tracking via `Metrics` struct

## Critical Integration Points

### s3dlio Library Integration
**Primary data path**: Uses s3dlio's `AsyncPoolDataLoader` with `MultiBackendDataset`
```rust
// Pattern: Always create object store through s3dlio
let store = store_for_uri(&config.storage_uri()).await?;
let loader = AsyncPoolDataLoader::new(dataset, pool_config).await?;
```

### Credential Management
- **S3**: Uses `.env` file loading via `dotenvy::dotenv()` in WorkloadRunner
- **Azure**: Environment variables `AZURE_BLOB_ACCOUNT`, `AZURE_BLOB_CONTAINER`
- **Pattern**: Credential loading happens in WorkloadRunner constructor

### Testing Patterns
- **Conditional tests**: S3/Azure tests skip if credentials missing
- **Environment checks**: `env::var("S3_ENDPOINT").is_err()` pattern for conditional execution
- **Config-driven**: All tests use YAML configs from `tests/configs/`
- **MLCommons validation**: Comprehensive DLIO config tests in `tests/mlcommons_dlio_validation.rs`
- **Real benchmark configs**: Test suite includes actual UNet3D, BERT, ResNet, CosmoFlow configurations

## Development Workflows

### Build & Test Commands
```bash
# Build entire workspace
cargo build --release

# Run all tests (some require credentials)
cargo test

# Test specific backend with credentials
AZURE_BLOB_ACCOUNT=myaccount cargo test test_azure_backend
S3_ENDPOINT=http://localhost:9000 cargo test test_s3_backend

# Run workload with legacy config
./target/release/dl-driver legacy --config tests/configs/test_file_config.yaml

# Run workload with MLCommons DLIO config
./target/release/dl-driver dlio --config tests/dlio_configs/minimal_config.yaml

# Validate DLIO config without running
./target/release/dl-driver validate --config tests/dlio_configs/unet3d_config.yaml

# Run MLCommons validation tests
cargo test --test mlcommons_dlio_validation
```

### Configuration Examples
**File backend**: `data_folder: file:///tmp/workload/`
**S3 backend**: `data_folder: s3://bucket/path/` (needs AWS credentials)
**Azure backend**: `data_folder: az://account/container/path/` (needs AZURE_BLOB_ACCOUNT)
**DirectIO**: `data_folder: direct:///tmp/high-perf/` (O_DIRECT for HPC)

### DLIO Config Examples
**Minimal workload**: See `tests/dlio_configs/minimal_config.yaml`
**UNet3D benchmark**: See `tests/dlio_configs/unet3d_config.yaml` 
**BERT benchmark**: See `tests/dlio_configs/bert_config.yaml`
**ResNet benchmark**: See `tests/dlio_configs/resnet_config.yaml`
**CosmoFlow benchmark**: See `tests/dlio_configs/cosmoflow_config.yaml`

## Project-Specific Conventions

### Error Handling
- Use `anyhow::Result` everywhere, with `.context()` for error chains
- Storage errors bubble up from s3dlio ObjectStore operations
- Pattern: `store.put(&path, &data).await.with_context(|| format!("Failed to write {}", path))?`

### Async Patterns
- **Runtime**: All main execution uses `#[tokio::main]` 
- **Storage I/O**: Always async through s3dlio ObjectStore
- **Test functions**: Use `#[tokio::test]` for integration tests

### Naming Conventions
- **Configs**: `test_[backend]_config.yaml` pattern
- **File generation**: `train_file_{:06}.{format}` numbering
- **Metrics**: Track files_processed, bytes_read/written, execution times

### Performance Considerations
- **Batch operations**: Use s3dlio's PoolConfig for concurrent I/O tuning
- **Memory efficiency**: Stream processing preferred over loading entire files
- **Backend optimization**: DirectIO for HPC, async pools for cloud storage

## Common Troubleshooting

**Missing credentials**: Tests will skip rather than fail
**Path resolution**: Always use full URIs, handle trailing slash inconsistencies
**s3dlio dependency**: Core functionality depends on external s3dlio crate - don't reimplement storage logic
**Format support**: Currently NPZ-focused, HDF5 planned but not fully implemented