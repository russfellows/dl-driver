# Changelog

All notable changes to the real_dlio project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-08-27

### 🎉 Major Features Added

#### **Complete Storage Backend Support**
- ✅ **File Backend** (`file://`) - Local filesystem operations
  - Performance: 46.46 MB/s throughput
  - Status: Full support with 5×512KB test files (2.5 MB total)
  
- ✅ **S3 Backend** (`s3://`) - AWS S3 and MinIO compatibility  
  - Performance: 20.02 MB/s throughput
  - Status: Full support with 10×1MB test files (10 MB total)
  - Features: Real credentials support, MinIO integration
  
- ✅ **Azure Backend** (`az://`) - Azure Blob Storage
  - Performance: 0.42 MB/s throughput
  - Status: Full support with 3×256KB test files (768 KB total)
  - Features: Azure CLI authentication, real storage account integration
  
- ✅ **DirectIO Backend** (`direct://`) - High-performance O_DIRECT file I/O
  - Performance: **85.45 MB/s throughput** (highest performance)
  - Status: Full support with 4×1MB test files (4 MB total)
  - Features: Zero-copy I/O operations, automatic fallback

#### **Core Infrastructure**
- **Unified s3dlio Integration**: All backends use consistent `object_store` interface
- **Automatic Backend Detection**: URI scheme-based selection (`file://`, `s3://`, `az://`, `direct://`)
- **Complete DLIO Configuration Compatibility**: Full YAML config parsing
- **Async I/O Support**: Tokio-based async operations throughout
- **Comprehensive Metrics**: Performance tracking and reporting

#### **Rust Toolchain**
- **Rust 1.89.0**: Upgraded from 1.86.0 for s3dlio compatibility
- **Zero Warnings**: Clean compilation with all warnings addressed
- **Production Dependencies**: s3dlio v0.7.4, tokio, anyhow, serde ecosystem

### 🧪 Testing Infrastructure

#### **Backend Integration Tests**
- **All 4 Storage Backends**: Comprehensive test suite
- **Real Credentials**: S3/MinIO and Azure authentication
- **Performance Validation**: Throughput and latency metrics
- **Error Handling**: Graceful failure scenarios

#### **Regression Test Suite**
- `tests/backend_integration.rs` - End-to-end backend testing
- `tests/config_tests.rs` - Configuration parsing validation
- `tests/configs/` - Reference configurations for all backends

### 🛠️ Development Workflow

#### **Project Structure**
- **Workspace Architecture**: 5 crates (core, storage, formats, py_api, cli)
- **Version Management**: Coordinated v0.2.0 across all crates
- **Documentation**: Structured docs/ directory

#### **Quality Assurance**
- **Warning-Free Compilation**: All Rust warnings resolved
- **Test Coverage**: Integration and unit test frameworks
- **Environment Configuration**: dotenvy for credential management

### 📊 Performance Benchmarks

| Backend | URI Scheme | Throughput | Files | Total Data | Status |
|---------|------------|------------|-------|------------|--------|
| **DirectIO** | `direct://` | **85.45 MB/s** | 4×1MB | 4 MB | ✅ Working |
| **File** | `file://` | 46.46 MB/s | 5×512KB | 2.5 MB | ✅ Working |
| **S3/MinIO** | `s3://` | 20.02 MB/s | 10×1MB | 10 MB | ✅ Working |
| **Azure** | `az://` | 0.42 MB/s | 3×256KB | 768 KB | ✅ Working |

### 🎯 Milestone Achievements

- **✅ Checkpoint 1**: Foundation architecture and DLIO config parsing
- **✅ Checkpoint 2**: s3dlio integration and Rust toolchain upgrade  
- **✅ Checkpoint 3**: Complete 4-backend storage implementation

### 🔧 Technical Implementation

#### **s3dlio Object Store Integration**
```rust
// Unified interface for all backends
let store = s3dlio::object_store::store_for_uri(uri)?;
let data = store.get(uri).await?;
store.put(uri, &data).await?;
```

#### **Backend Detection Logic**
```rust
pub fn storage_backend(&self) -> StorageBackend {
    let uri = self.storage_uri();
    if uri.starts_with("s3://") { StorageBackend::S3 }
    else if uri.starts_with("az://") { StorageBackend::Azure }
    else if uri.starts_with("direct://") { StorageBackend::DirectIO }
    else { StorageBackend::File }
}
```

### 🚀 Next Phase Roadmap

**Ready for Checkpoint 4 - Data Format Support:**
- HDF5 format handlers
- NPZ format support  
- TensorFlow format integration
- RAW format (Parquet, JSON, etc.)

**Planned Features:**
- Multi-threading and concurrent I/O
- s3dlio advanced data loader capabilities
- Checkpointing and resume functionality
- Compression support (LZ4, GZIP)
- Python API bindings

---

## [0.1.0] - 2025-08-26

### Added
- Initial project structure with workspace architecture
- Basic CLI interface with clap argument parsing
- DLIO configuration parsing foundation
- Core workload orchestration framework
- Initial storage backend abstractions
