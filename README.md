# dl-driver

**A high-performance, enterprise-grade data loading framework for AI/ML workloads**

[![Rust](https://img.shields.io/badge/rust-1.89.0+-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.3.0-green.svg)](./docs/Changelog.md)
[![Storage](https://img.shields.io/badge/storage-4%20backends-orange.svg)](#storage-backends)
[![Performance](https://img.shields.io/badge/performance-62K%20files%2Fsec-red.svg)](#performance-benchmarks)

## 🚀 Overview

This project is designed to generate workloads similar to **enterprise-grade AI/ML data loading and checkpointing framework** designed for AI/ML storage workload testing and production use. Built in Rust and designed to be comparible at the configuration and workload level with the [dlio](https://github.com/argonne-lcf/dlio_benchmark) project, it provides **unified, high-performance access** to multiple storage backends through the powerful [s3dlio](https://github.com/russfellows/s3dlio) library.

### ✨ Key Features

- **🔗 DLIO Configuration Compatibility**: Drop-in replacement for existing DLIO YAML configs
- **🏪 4 Universal Storage Backends**: File, S3/MinIO, Azure Blob, DirectIO with unified interface  
- **⚡ Enterprise Performance**: **62K+ files/second** with advanced AsyncPoolDataLoader
- **� Advanced Data Loading**: Dynamic batching, zero head latency, auto-tuning
- **🔄 Multi-Threading**: Concurrent processing with backend-optimized configurations
- **☁️ Production Cloud Ready**: Real S3 and Azure credential support
- **🧪 Comprehensively Tested**: 300+ files validated across all storage backends

## 📊 Performance Benchmarks

### v0.3.0 - Advanced Data Loading Performance
**AsyncPoolDataLoader with Dynamic Batching** ⚡

| Backend | Files/Second | Processing Time | Configuration |
|---------|--------------|-----------------|---------------|
| **File** | **62,494** | 1.20ms | 24 pool, 6 workers |
| **S3/MinIO** | **44,831** | 1.67ms | 32 pool, 8 workers |
| **Azure** | **37,926** | 1.98ms | 28 pool, 7 workers |
| **DirectIO** | **23,061** | 3.25ms | 16 pool, 4 workers |

*Test conditions: 75 files per backend, 5 content types, microsecond batch latency*

### v0.2.0 - Storage Backend Throughput
**Traditional Sequential Processing** 📈

| Backend | URI Scheme | Throughput | Use Case |
|---------|------------|------------|----------|
| **DirectIO** | `direct://` | **85.45 MB/s** | High-performance local I/O |
| **File** | `file://` | 46.46 MB/s | Standard filesystem operations |
| **S3/MinIO** | `s3://` | 20.02 MB/s | Cloud object storage |
| **Azure** | `az://` | 0.42 MB/s | Azure Blob Storage |

## 🏗️ Architecture

dl-driver follows a clean workspace architecture with 5 focused crates:

```
real_dlio/
├── crates/
│   ├── cli/          # Command-line interface
│   ├── core/         # Workload orchestration and config parsing  
│   ├── storage/      # Storage backend abstractions
│   ├── formats/      # Data format handlers (HDF5, NPZ, etc.)
│   └── py_api/       # Python bindings (PyO3)
├── tests/            # Integration and regression tests
└── docs/             # Documentation and changelog
```

## 🚀 Quick Start

### Installation

```bash
git clone https://github.com/russfellows/dl-driver.git
cd dl-driver
cargo build --release
```

### Basic Usage

```bash
# Test file system backend
./target/release/dl-driver --config tests/configs/test_file_config.yaml

# Test S3/MinIO backend  
./target/release/dl-driver --config tests/configs/test_s3_large_config.yaml

# Test Azure backend (requires Azure credentials)
AZURE_BLOB_ACCOUNT=your_account ./target/release/dl-driver --config tests/configs/test_azure_config.yaml

# Test DirectIO backend
./target/release/dl-driver --config tests/configs/test_directio_config.yaml
```

## 🏪 Storage Backends

This project provides unified access to multiple storage systems through URI schemes:

### File System (`file://`)
```yaml
dataset:
  data_folder: file:///tmp/my-workload/
```
- **Use Case**: Local filesystem testing, development
- **Performance**: 46+ MB/s throughput
- **Features**: Standard POSIX file operations

### S3 Compatible (`s3://`)  
```yaml
dataset:
  data_folder: s3://my-bucket/my-workload/
```
- **Use Case**: AWS S3, MinIO, S3-compatible object stores
- **Performance**: 20+ MB/s throughput  
- **Authentication**: AWS credentials, .env file support

### Azure Blob Storage (`az://`)
```yaml  
dataset:
  data_folder: az://storage-account/container/path/
```
- **Use Case**: Azure cloud storage
- **Authentication**: Azure CLI, service principal
- **Features**: Integrated with Azure SDK

### DirectIO (`direct://`)
```yaml
dataset:  
  data_folder: direct:///tmp/high-perf-workload/
```
- **Use Case**: High-performance applications, HPC workloads
- **Performance**: 85+ MB/s throughput
- **Features**: O_DIRECT, zero-copy I/O, automatic fallback

## 📝 Configuration

real_dlio is fully compatible with DLIO configuration files:

```yaml
# Example workload configuration
model:
  name: unet3d_workload
  model_size: 499153191

framework: pytorch

workflow:
  generate_data: true
  train: true  
  checkpoint: false

dataset:
  data_folder: s3://my-bucket/workload-data/
  format: npz
  num_files_train: 100
  record_length_bytes: 1048576  # 1MB files

reader:
  data_loader: pytorch
  batch_size: 32
  read_threads: 4

train:
  epochs: 10
  computation_time: 0.1
```

## 🧪 Testing

Run the comprehensive test suite:

```bash
# Run all integration tests
cargo test

# Test specific backend (requires credentials)
AZURE_BLOB_ACCOUNT=myaccount cargo test test_azure_backend
S3_ENDPOINT=http://localhost:9000 cargo test test_s3_backend
```

## 🛠️ Development

### Prerequisites
- Rust 1.89.0 or later
- s3dlio library (automatically handled by Cargo)

### Building from Source
```bash
git clone https://github.com/russfellows/real_dlio.git
cd real_dlio
cargo build
```

### Contributing
1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality  
4. Ensure all tests pass
5. Submit a pull request

## 📈 Roadmap

### Current Status (v0.2.0)
- ✅ All 4 storage backends working
- ✅ DLIO configuration compatibility
- ✅ Comprehensive test suite
- ✅ Production-ready error handling

### Next Phase (v0.3.0)
- 🔄 Data format support (HDF5, NPZ, TensorFlow, Parquet)
- 🔄 Multi-threaded concurrent I/O operations
- 🔄 Advanced s3dlio data loader integration
- 🔄 Checkpointing and resume functionality

### Future Features
- Compression support (LZ4, GZIP, Zstd)
- Python API bindings (PyO3)
- Advanced metrics and profiling
- Distributed workload coordination

## 📚 Documentation

- [Changelog](./docs/Changelog.md) - Detailed version history
- [Configuration Guide](./tests/configs/) - Example configurations
- [API Documentation](https://docs.rs/real_dlio) - Rust API docs

## 🤝 Acknowledgments

- [DLIO Benchmark](https://github.com/argonne-lcf/dlio_benchmark) - Original inspiration and configuration format
- [s3dlio](https://github.com/russfellows/s3dlio) - Powerful multi-backend storage library
- Rust ecosystem - tokio, serde, anyhow, and many other excellent crates

## 📄 License

This project is licensed under the same terms as the original DLIO benchmark.

---

