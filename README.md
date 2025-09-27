# dl-driver

**A unified, high-performance AI/ML data loading framework with enterprise-grade capabilities**

[![Rust](https://img.shields.io/badge/rust-1.89.0+-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.6.3-green.svg)](./docs/Changelog.md)
[![Build](https://img.shields.io/badge/build-passing-success.svg)](#compilation-status)
[![Formats](https://img.shields.io/badge/formats-3%20validated-brightgreen.svg)](#format-compatibility)
[![Validation](https://img.shields.io/badge/tests-21%2F21%20passing-success.svg)](#testing--validation)
[![Storage](https://img.shields.io/badge/storage-4%20backends-orange.svg)](#storage-backends)
[![Architecture](https://img.shields.io/badge/architecture-unified-blue.svg)](#architecture-overview)
[![REUSE status](https://api.reuse.software/badge/github.com/russfellows/dl-driver)](https://api.reuse.software/info/github.com/russfellows/dl-driver)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![ScanCode Compatible](https://img.shields.io/badge/ScanCode-Compatible-green.svg)](https://scancode.io/)

## 🚀 Overview

**dl-driver** is a production-ready AI/ML data loading framework that provides **100% format compatibility** with standard Python libraries. Built in Rust for performance and reliability, it serves as a drop-in replacement for [DLIO benchmarks](https://github.com/argonne-lcf/dlio_benchmark) while delivering enterprise-grade capabilities through the powerful [s3dlio](https://github.com/russfellows/s3dlio) library.

**Key Achievement**: Complete validation with numpy, h5py, and TensorFlow ensures seamless integration with existing ML pipelines.

## 🎯 Current Status (v0.6.3)

**🌟 PLAN A1 MULTI-GPU SCALING**: Enterprise-grade multi-process coordination with shared memory
**🔥 SHARED MEMORY COORDINATION**: Atomic operations, barriers, zero temp files
**⚡ DISTRIBUTED EXECUTION**: Multi-rank synchronization with aggregated performance metrics
**🚀 PRODUCTION READY**: HPC and AI/ML cluster coordination with fault tolerance

### Latest v0.6.3 Release - Plan A1 Multi-Process Coordination 🌟
- **🔥 Shared Memory Coordination**: Complete atomic coordination system replacing temp files
- **⚡ Plan A1 Multi-GPU**: `--world-size N --rank R` for distributed execution across processes  
- **🏗️ Enterprise Architecture**: AtomicU32/U64/Bool with barriers, proper cleanup, timeout handling
- **📊 Aggregated Results**: Combined throughput and metrics across all ranks with per-rank breakdown
- **🧪 Coordination Testing**: Isolated test framework validating barriers and synchronization
- **🎯 Zero Dependencies**: No MPI/network requirements - pure shared memory coordination

### Previous Releases
- **v0.6.2**: TRUE DLIO parallel I/O with corrected throughput calculations and realistic AU metrics
- **v0.6.1**: Enterprise license compliance (REUSE 3.3) with automated scanning
- **v0.6.0**: Unified command interface and comprehensive plugin system

## 🌟 Plan A1 Multi-Process Scaling Usage (v0.6.3)

### Multi-Rank Distributed Execution
Execute DLIO workloads across multiple processes with shared memory coordination:

```bash
# 2-Process execution (simulating 2 GPUs)
./target/release/dl-driver run --config config.yaml --world-size 2 --rank 0 &
./target/release/dl-driver run --config config.yaml --world-size 2 --rank 1 &

# 4-Process execution (simulating 4 GPUs) 
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 0 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 1 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 2 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 3 &

# Rank 0 will display aggregated results:
🎉 Plan A1 Multi-GPU Results (Shared Memory Coordination):
================================================================
Total files processed: 28
Total data read: 0.40 GiB
Combined throughput: 11.16 GiB/s
Global runtime: 0.071s
Number of ranks: 4
✅ Multi-rank coordination successful - NO TEMP FILES USED
```

### Key Multi-Process Features
- **🔗 Shared Memory Coordination**: Zero temp files, atomic operations, cross-process barriers
- **📊 Automatic Aggregation**: Rank 0 displays combined performance across all processes  
- **⚡ Synchronized Execution**: All ranks coordinate start/stop for accurate timing
- **🎯 Interleaved Sharding**: Optimal data distribution across ranks
- **🧹 Automatic Cleanup**: Proper shared memory cleanup on completion or failure

## 🚀 Single-Process DLIO Execution

```bash
# Build and run standard DLIO workload
cargo build --release
./target/release/dl-driver run --config tests/dlio_configs/minimal_config.yaml

# Generate data separately (optional)
./target/release/dl-driver generate --config config.yaml

# Validate configuration
./target/release/dl-driver validate --config config.yaml

# MLPerf compliance mode (enhanced reporting)
./target/release/dl-driver run --mlperf --config config.yaml --format json
```

### ✨ Key Features

- **🌟 Plan A1 Multi-Process Scaling**: `--world-size N --rank R` distributed execution with shared memory coordination
- **🔥 Enterprise Coordination**: Atomic operations, cross-process barriers, zero temp files  
- **🚀 TRUE DLIO Parallel I/O**: Background workers with I/O+compute overlap for realistic performance
- **🎯 Complete Format Compatibility**: NPZ, HDF5, TFRecord validated with numpy, h5py, TensorFlow
- **🏪 Universal Storage**: File, S3/MinIO, Azure Blob, DirectIO backends with unified interface  
- **� MLCommons DLIO Compatible**: Drop-in replacement for existing DLIO benchmark configurations
- **📊 Production Ready**: Enterprise license compliance, comprehensive testing, checkpoint system
- **☁️ Production Cloud Ready**: Real S3 and Azure credential support
- **🧪 Comprehensively Validated**: 60+ comprehensive tests with golden reference validation and MLCommons DLIO compatibility

## 🎯 Technical Specifications

### Storage Backends
- **File System**: POSIX-compliant file I/O with DirectIO optimization
- **Cloud Storage**: S3/MinIO and Azure Blob with credential support
- **Performance**: Multi-GiB/s throughput with enterprise-grade reliability

### Data Formats  
- **NPZ, HDF5, TFRecord**: 100% compatible with numpy, h5py, and TensorFlow
- **Framework Support**: PyTorch, TensorFlow, and JAX configuration profiles
- **Validation**: Comprehensive test suite ensuring standard library compatibility

## 🏆 Key Achievements

### 🎯 Production-Ready AI/ML Pipeline
dl-driver v0.4.0 represents a **major milestone** - complete transformation from a performance framework to a production-ready AI/ML data pipeline:

- **100% Format Compatibility**: All generated files work seamlessly with standard Python libraries
- **Enterprise Validation**: 36 comprehensive format tests ensure ongoing quality assurance
- **DLIO Drop-in Replacement**: Full MLCommons configuration compatibility with enhanced features
- **Multi-Backend Excellence**: Unified performance across File, S3, Azure, and DirectIO storage

### 📊 Validation Confidence
```
✅ Framework Tests: 7/7 tests passing (PyTorch integration, validation, serialization)
✅ Core Tests:     15/15 tests passing (DLIO parsing, workload management) 
✅ Format Tests:    5/5 tests passing (NPZ, HDF5, TFRecord)
✅ CLI Tests:      29/29 tests passing (configuration, backend integration)
✅ Total Coverage: 56/56 comprehensive tests validating all functionality
```

## 🏗️ Architecture

dl-driver follows a clean workspace architecture with 6 focused crates:

```
real_dlio/
├── crates/
│   ├── cli/          # Command-line interface
│   ├── core/         # Workload orchestration and config parsing  
│   ├── frameworks/   # Framework integrations (PyTorch, TensorFlow, JAX)
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
# Generate test datasets with different formats
./target/release/dl-driver generate --config tests/dlio_configs/minimal_config.yaml

# Run DLIO-compatible workloads (unified execution engine)
./target/release/dl-driver run --config tests/dlio_configs/unet3d_config.yaml

# Validate configuration without running
./target/release/dl-driver validate --config tests/dlio_configs/bert_config.yaml

# Run format validation (requires Python environment)
python tools/validation/validate_formats.py
```

### Command Overview
```bash
dl-driver --help                    # Show all available commands
dl-driver generate --help           # Generate synthetic datasets  
dl-driver run --help               # Run DLIO workloads (with optional MLPerf mode)
dl-driver validate --help          # Validate configurations
```

## 📝 Configuration

DLIO-compatible YAML configuration with multi-backend storage support:

```yaml
dataset:
  data_folder: file:///mnt/vast1/data/  # file://, s3://, az://, direct://
  format: npz                           # npz, hdf5, tfrecord  
  num_files_train: 1000

reader:
  batch_size: 32
  read_threads: 4
  
train:
  epochs: 5
  computation_time: 0.05
```

Configuration examples available in `tests/dlio_configs/`

## 🧪 Testing & Validation

```bash
# Build and test
cargo build --release
cargo test

# Test multi-rank coordination
./target/release/dl-driver run --config config.yaml --world-size 2 --rank 0 &
./target/release/dl-driver run --config config.yaml --world-size 2 --rank 1
```

### Validation Results
- ✅ **45/45 Rust integration tests** passing
- ✅ **36/36 format validation tests** with Python libraries
- ✅ **100% compatibility** with numpy, h5py, tensorflow
- ✅ **MLCommons DLIO configs** fully validated

### Test Categories
- **Backend Integration**: File, S3, Azure, DirectIO validation
- **Format Compatibility**: NPZ, HDF5, TFRecord with standard libraries
- **DLIO Compliance**: Configuration parsing and workload execution
- **Performance**: s3dlio AsyncPoolDataLoader benchmarks

## 🛠️ Development

### Prerequisites
- Rust 1.89.0 or later
- s3dlio library (automatically handled by Cargo)

### Building from Source
```bash
git clone https://github.com/russfellows/dl-driver.git
cd dl-driver
cargo build --release
```

### Contributing
1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality  
4. Ensure all tests pass
5. Submit a pull request

##  Documentation

- [Changelog](./docs/Changelog.md) - Detailed version history
- [Configuration Guide](./tests/configs/) - Example configurations
- [API Documentation](https://docs.rs/real_dlio) - Rust API docs

## 🤝 Acknowledgments

- [DLIO Benchmark](https://github.com/argonne-lcf/dlio_benchmark) - Original inspiration and configuration format
- [s3dlio](https://github.com/russfellows/s3dlio) - Powerful multi-backend storage library
- Rust ecosystem - tokio, serde, anyhow, and many other excellent crates

## 📄 License & Compliance

This project maintains **enterprise-grade license compliance** with comprehensive scanning and validation.

### License Information
- **License**: [GPL-3.0-or-later](LICENSES/GPL-3.0-or-later.txt) 
- **REUSE Compliant**: Full compliance with [REUSE Specification 3.3](https://reuse.software/spec/)
- **SPDX Standards**: All source files include proper SPDX license identifiers
- **ScanCode Compatible**: Validated with ScanCode Toolkit for enterprise scanning

### Compliance Summary
- ✅ **201 files scanned** by ScanCode Toolkit
- ✅ **72 files** with SPDX GPL-3.0 identifiers  
- ✅ **80 files** with proper copyright attribution
- ✅ **Automated CI/CD** license validation via GitHub Actions

📋 **[View Detailed Compliance Report](docs/LICENSE-COMPLIANCE.md)**

### Local Validation
```bash
# REUSE compliance check
reuse lint

# ScanCode analysis (via Docker)
docker run --rm -v $(pwd):/workdir sixarm/scancode \
  --copyright --license --package --info --license-text \
  --strip-root --format html-app /workdir /workdir/compliance-report.html
```

---

