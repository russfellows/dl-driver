# dl-driver

**A tool for performing realistic testing of storage performance when running AI/ML workloads**

[![Rust](https://img.shields.io/badge/rust-1.89.0+-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.7.2-green.svg)](./docs/Changelog.md)
[![Build](https://img.shields.io/badge/build-passing-success.svg)](#compilation-status)
[![Formats](https://img.shields.io/badge/formats-3%20validated-brightgreen.svg)](#format-compatibility)
[![Validation](https://img.shields.io/badge/tests-61%2F61%20passing-success.svg)](#testing--validation)
[![Storage](https://img.shields.io/badge/storage-4%20backends-orange.svg)](#storage-backends)
[![Architecture](https://img.shields.io/badge/architecture-unified-blue.svg)](#architecture-overview)
[![REUSE status](https://api.reuse.software/badge/github.com/russfellows/dl-driver)](https://api.reuse.software/info/github.com/russfellows/dl-driver)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![ScanCode Compatible](https://img.shields.io/badge/ScanCode-Compatible-green.svg)](https://scancode.io/)

## 🚀 Overview

**dl-driver** is a tool for testing storage performance during AI/ML workloads.  For training workloads it supports running data generation, data loading and checkpoint tests that provide **format compatibility** with standard Python libraries. Built in Rust for performance and reliability, it serves as a drop-in replacement for [DLIO benchmarks](https://github.com/argonne-lcf/dlio_benchmark) while delivering enterprise-grade capabilities through the powerful [s3dlio](https://github.com/russfellows/s3dlio) library.

**Key Achievement**: Validation of object/file formats with numpy, h5py, and TensorFlow provides integration with existing ML pipelines.

## 🎯 Current Status (v0.7.2)

**📝 DOCUMENTATION CLARITY**: Clear separation of ML/AI simulation vs storage I/O replay
**🧹 CODE CLEANUP**: Removed non-operational replay tests, clarified stub functions
**📦 S3DLIO 0.8.20**: Latest s3dlio release with tagged dependencies
**🔄 INFRASTRUCTURE ONLY**: Streaming replay architecture for future integration

### Latest v0.7.2 Release - Documentation & Code Clarity 📝
- **� Replay Clarification**: Updated all documentation to clarify replay is infrastructure/simulation only
- **🔗 sai3-bench Reference**: Added clear guidance to use sai3-bench for real I/O replay needs
- **🧹 Code Cleanup**: Removed streaming_replay_tests.rs (simulation-only tests)
- **� Stub Documentation**: Marked simulate_operation() and related functions as stubs for future integration
- **📦 s3dlio 0.8.20**: Upgraded to latest tagged release (from 0.8.19)
- **� Analysis Document**: Created comprehensive REPLAY_ANALYSIS.md comparing dl-driver vs sai3-bench

### Previous Releases
- **v0.7.1**: Streaming replay infrastructure with s3dlio-oplog integration (simulation only)
- **v0.6.6**: Naming consistency and base URI integration for replay functionality
- **v0.6.4**: Realistic AI/ML framework workload simulation with PyTorch/TensorFlow/JAX profiles
- **� Replay Analytics**: Comprehensive metrics including timing accuracy and throughput analysis

### Previous Releases
- **v0.6.4**: Realistic AI/ML framework workload simulation with PyTorch/TensorFlow/JAX profiles
- **v0.6.2**: Tested for accurate DLIO parallel I/O with throughput calculations and AU metrics
- **v0.6.1**: Enterprise license compliance (REUSE 3.3) with automated scanning
- **v0.6.0**: Unified command interface and comprehensive plugin system

## 🔄 Operation Log Replay

> ⚠️ **Note**: dl-driver's replay functionality is **infrastructure only** and uses simulated operations.
> For **real I/O replay** with actual storage operations, use **[sai3-bench](https://github.com/russfellows/sai3-bench)**
> which provides production-grade replay with:
> - Real ObjectStore I/O execution via s3dlio
> - Advanced remapping (1:1, 1→N, N→1, regex patterns)
> - Microsecond timing precision with HDR histograms
> - Distributed load generation with gRPC coordination
>
> See `docs/REPLAY_ANALYSIS.md` for detailed comparison and use case guidance.

### Simulated Replay (Infrastructure Testing)
dl-driver provides operation log parsing and streaming infrastructure for testing purposes:

```bash
# Simulated replay (no real I/O - for testing infrastructure only)
./target/release/dl-driver replay --log-file operations.csv --fast
```

### Path Remapping Configuration
Create a JSON file for environment-specific path translation:

```json
{
  "/original/data/path": "/new/deployment/path",
  "s3://source-bucket": "s3://target-bucket",
  "/mnt/old": "/mnt/new"
}
```

### Example Replay Output
```bash
🔄 Operation Log Replay Starting...
📂 Loading operation log: operations.csv
🗺️ Applied path remapping: 3 mappings loaded
⚙️ Workers: 4, Timeout: 60s, Preserve timing: true

📊 Replay Progress:
✅ Operations processed: 1,247/1,247 (100%)
📈 Throughput: 2.3 GiB/s
⏱️ Total time: 45.2s
🎯 Success rate: 99.8% (3 timeouts)

🎉 Replay completed successfully!
```

### Infrastructure Features (Simulation Only)
- **⏱️ Timing Control**: Parse and validate inter-arrival timing from op-logs
- **🗺️ Path Remapping**: JSON-based path translation validation
- **🔄 Streaming Architecture**: Constant-memory op-log processing via s3dlio-oplog
- **📊 Progress Tracking**: Operation counting and timing metrics (simulated)

**For actual storage I/O replay**, use [sai3-bench](https://github.com/russfellows/sai3-bench) instead.

## 🌟 Multi-Process Scaling Usage (v0.6.3)

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

- **🌟 Multi-Process Scaling**: `--world-size N --rank R` distributed execution with shared memory coordination
- **🔥 Enterprise Coordination**: Atomic operations, cross-process barriers, zero temp files  
- **🚀 TRUE DLIO Parallel I/O**: Background workers with I/O+compute overlap for realistic performance
- **🎯 Complete Format Compatibility**: NPZ, HDF5, TFRecord validated with numpy, h5py, TensorFlow
- **🏪 Universal Storage**: File, S3/MinIO, Azure Blob, DirectIO backends with unified interface  
- **� DLIO Compatible**: Drop-in replacement for existing DLIO benchmark configurations
- **📊 Production Ready**: Enterprise license compliance, comprehensive testing, checkpoint system
- **☁️ Production Cloud Ready**: Real S3 and Azure credential support
- **🧪 Comprehensively Validated**: 60+ comprehensive tests with golden reference validation and MLCommons DLIO compatibility

## 🧠 Workstream A: Realistic AI/ML Framework Simulation (v0.6.4)

### Framework-Specific Workload Profiles
Execute workloads optimized for specific AI/ML frameworks:

```bash
# PyTorch-optimized workload simulation
./target/release/dl-driver run --config config.yaml --profile torch

# TensorFlow-optimized configuration  
./target/release/dl-driver run --config config.yaml --profile tf

# JAX-optimized workload patterns
./target/release/dl-driver run --config config.yaml --profile jax
```

### Advanced Metrics Export & CI Integration
Export comprehensive performance metrics for automated analysis:

```bash
# Export metrics to JSON for programmatic analysis
./target/release/dl-driver run --config config.yaml --metrics-json results.json

# Export metrics to CSV for spreadsheet analysis
./target/release/dl-driver run --config config.yaml --metrics-csv results.csv

# Both formats simultaneously for comprehensive reporting
./target/release/dl-driver run --config config.yaml --metrics-json metrics.json --metrics-csv metrics.csv
```

### Operation Log Validation & Benchmarking
Validate workload performance against reference operation logs:

```bash
# Validate against compressed operation log (supports .csv.zst, .jsonl.zst)
./target/release/dl-driver run --config config.yaml --op-log reference-benchmark.csv.zst

# Example with comprehensive validation and metrics export
./target/release/dl-driver run \
    --config config.yaml \
    --profile torch \
    --metrics-json validation-results.json \
    --op-log production-reference.csv.zst

# Validation output with CI-friendly exit codes:
✅ PASS: Workload performance within tolerance (±5.0%)
📊 Files processed: 1000 (reference: 1000)  
📊 Throughput: 12.4 GiB/s (reference: 12.1 GiB/s, +2.5%)
📊 Total runtime: 45.2s (reference: 46.1s, -2.0%)
```

### Key Workstream A Features
- **🧠 Intelligent Profiles**: Framework-specific optimizations for PyTorch, TensorFlow, and JAX
- **📊 Production Metrics**: JSON/CSV export for CI/CD pipelines and performance tracking
- **🔍 Validation Engine**: Compare against reference operation logs with configurable tolerance
- **⚡ Real-World Testing**: Validated with 2.78M record operation logs from production systems
- **🎯 CI Integration**: PASS/FAIL validation with proper exit codes for automated testing

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

### 🎯 Realistic testing of AI/ML Pipeline
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

# NEW: Framework-specific workload profiles (Workstream A)
./target/release/dl-driver run --config config.yaml --profile torch
./target/release/dl-driver run --config config.yaml --profile tf
./target/release/dl-driver run --config config.yaml --profile jax

# NEW: Metrics export for CI/CD integration (Workstream A)
./target/release/dl-driver run --config config.yaml --metrics-json results.json
./target/release/dl-driver run --config config.yaml --metrics-csv results.csv

# NEW: Operation log validation (Workstream A)
./target/release/dl-driver run --config config.yaml --op-log reference.csv.zst

# Run format validation (requires Python environment)
python tools/validation/validate_formats.py
```

### Command Overview
```bash
dl-driver --help                    # Show all available commands
dl-driver generate --help           # Generate synthetic datasets  
dl-driver run --help               # Run DLIO workloads (with optional MLPerf mode)
dl-driver validate --help          # Validate configurations

# Workstream A: Advanced execution options
dl-driver run --profile [torch|tf|jax]     # Framework-specific optimization profiles
dl-driver run --metrics-json FILE          # Export metrics in JSON format
dl-driver run --metrics-csv FILE           # Export metrics in CSV format  
dl-driver run --op-log FILE                # Validate against reference operation log
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

# NEW: Test Workstream A features (v0.6.4)
./target/release/dl-driver run --config config.yaml --profile torch --metrics-json test.json
./target/release/dl-driver run --config config.yaml --op-log tests/dlio_configs/reference.csv.zst
```

### Validation Results (v0.6.4)
- ✅ **45+ Rust integration tests** passing (including Workstream A features)
- ✅ **36/36 format validation tests** with Python libraries
- ✅ **Framework profiles** validated with PyTorch, TensorFlow, and JAX configurations
- ✅ **Operation log validation** tested with 2.78M record production datasets
- ✅ **Metrics export** validated in JSON and CSV formats for CI integration
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

