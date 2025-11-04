# dl-driver

**A tool for performing realistic testing of storage performance when running AI/ML workloads**

[![Rust](https://img.shields.io/badge/rust-1.89.0+-blue.svg)](https://www.rust-lang.org)
[![Version](https://img.shields.io/badge/version-0.8.4-green.svg)](./docs/Changelog.md)
[![Build](https://img.shields.io/badge/build-passing-success.svg)](#compilation-status)
[![Formats](https://img.shields.io/badge/formats-3%20validated-brightgreen.svg)](#format-compatibility)
[![Validation](https://img.shields.io/badge/tests-123%20passing-success.svg)](#testing--validation)
[![Storage](https://img.shields.io/badge/storage-4%20backends-orange.svg)](#storage-backends)
[![Distributed](https://img.shields.io/badge/distributed-multi--agent-purple.svg)](#distributed-execution)
[![Architecture](https://img.shields.io/badge/architecture-unified-blue.svg)](#architecture-overview)
[![REUSE status](https://api.reuse.software/badge/github.com/russfellows/dl-driver)](https://api.reuse.software/info/github.com/russfellows/dl-driver)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![ScanCode Compatible](https://img.shields.io/badge/ScanCode-Compatible-green.svg)](https://scancode.io/)

## 🚀 Overview

**dl-driver** is a tool for testing storage performance during AI/ML workloads.  For training workloads it supports running data generation, data loading and checkpoint tests that provide **format compatibility** with standard Python libraries. Built in Rust for performance and reliability, it serves as a drop-in replacement for [DLIO benchmarks](https://github.com/argonne-lcf/dlio_benchmark) while delivering enterprise-grade capabilities through the powerful [s3dlio](https://github.com/russfellows/s3dlio) library.

**Key Achievement**: Validation of object/file formats with numpy, h5py, and TensorFlow provides integration with existing ML pipelines.

## 🎯 Current Status

**🎉 v0.8.4 RELEASED**: Checkpoint reload functionality complete
**♻️ CHECKPOINT RELOAD**: Resume training from saved checkpoints with --resume-from-checkpoint flag
**🧪 COMPREHENSIVE TESTS**: Multi-backend checkpoint tests (file://, s3://, az://, gs://, direct://) 
**💾 CHECKPOINT SUPPORT**: Step-based and epoch-based checkpointing across all storage backends
**🔧 CLI SIMPLIFIED**: Removed legacy commands, unified interface with validate/--dry-run
**🎉 DISTRIBUTED CONTROLLER**: Multi-agent orchestration for true distributed workloads
**🌐 MULTI-NODE EXECUTION**: Coordinate workloads across multiple hosts with shared/local storage
**📊 HISTOGRAM AGGREGATION**: Accurate percentile calculation with <1% error for distributed workloads
**📁 RESULTS DIRECTORY**: Complete, reproducible results with per-agent and consolidated metrics
**✅ 123/123 TESTS PASSING**: Full validation across all features and backends

### Core Capabilities
- **♻️ Checkpoint Reload**: Resume training from saved checkpoints with automatic state restoration
- **💾 Checkpoint Plugin**: Step-based and epoch-based checkpointing with multi-backend support (file://, s3://, az://, gs://)
- **🔧 Clean CLI**: Unified interface with validate and --dry-run as aliases, legacy commands removed
- **🌐 Multi-Agent Orchestration**: Controller coordinates workloads across multiple agent instances
- **💓 Coordinated Start**: Synchronized workload execution with health checking
- **📊 Aggregate Metrics**: Automatic collection and aggregation from all agents with histogram-based percentiles
- **📁 Structured Results**: Complete results directory with per-agent TSV files and consolidated metrics
- **🗂️ Path Isolation**: Agent-specific path prefixes for local storage isolation
- **☁️ Shared Storage**: Automatic detection and handling of GCS/S3/Azure shared backends
- **✅ E2E Validated**: 2-node and 4-node configurations tested (local + cloud storage)
- **📈 Performance**: Multi-GiB/s aggregate throughput with accurate percentile tracking

**For storage I/O replay**, use [sai3-bench](https://github.com/russfellows/sai3-bench) instead.

## 📚 Documentation

**👉 For complete documentation, see [docs/USER_GUIDE.md](docs/USER_GUIDE.md)**

### Quick Links

- **[User Guide](docs/USER_GUIDE.md)** - Comprehensive guide covering all features
- **[Quick Start](docs/QUICK_START.md)** - Get started in minutes
- **[Distributed Setup](tests/dlio_configs/DISTRIBUTED_README.md)** - Multi-agent orchestration guide
- **[Changelog](docs/Changelog.md)** - Version history and release notes
- **[Dual Metrics](docs/DUAL_METRICS_REPORTING.md)** - Metrics specification
- **[Results Directory Format](docs/RESULTS_DIRECTORY_FORMAT.md)** - Structured results output specification

## 🌐 Distributed Execution

### Multi-Agent Orchestration
Execute DLIO workloads across multiple agent instances with centralized controller:

```bash
# Start agent processes on each host
# Host 1:
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 --bind-addr 0.0.0.0 &

# Host 2:
./target/release/dl_driver_agent --agent-id agent-1 --port 50051 --bind-addr 0.0.0.0 &

# Run distributed workload from controller
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_local.yaml \
  --agents http://host1:50051,http://host2:50051 \
  --path-template "{id}/"

# Output shows aggregated results:
╔════════════════════════════════════════════════╗
║   Distributed Workload Complete! 🎉           ║
╚════════════════════════════════════════════════╝

📊 Storage Performance (I/O Perspective):
   Total Throughput: 687.5 MiB/s
   Total Operations: 40
   Errors: 0

🤖 AI/ML Training Performance (Training Perspective):
   Training Velocity: 297.9 samples/s, 45.8 batches/s
   Pipeline Efficiency: 37.8%
```

### Storage Backend Modes

**Local Storage** (requires path template for agent isolation):
```bash
# Each agent writes to separate subdirectory
./target/release/dl-driver distributed run \
  --config distributed_local.yaml \
  --agents http://host1:50051,http://host2:50052 \
  --path-template "{id}/"
# Creates: /tmp/data/agent-0/, /tmp/data/agent-1/, etc.
```

**Shared Storage** (no path template needed):
```bash
# All agents write to same GCS/S3 bucket
./target/release/dl-driver distributed run \
  --config distributed_gcs.yaml \
  --agents http://host1:50051,http://host2:50052
# All write to: gs://bucket/distributed-test/
```

### Key Distributed Features
- **🌐 Multi-Host Orchestration**: Controller coordinates agents across network
- **💓 Health Checking**: Automatic agent health verification before execution
- **🔗 Coordinated Start**: Synchronized workload start across all agents
- **📊 Aggregate Metrics**: Automatic collection and aggregation from all agents
- **🗂️ Path Isolation**: Agent-specific subdirectories for local storage
- **☁️ Shared Storage**: Automatic detection of GCS/S3/Azure shared backends
- **📈 Dual Metrics**: Separate storage and AI/ML training perspectives

See `tests/dlio_configs/DISTRIBUTED_README.md` for complete usage guide.

## 🌟 Multi-Process Scaling Usage

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

- **🌐 Distributed Controller**: Multi-agent orchestration with coordinated start and histogram-based aggregate metrics
- **📁 Results Directory**: Complete, reproducible results with per-agent and consolidated TSV files
- **📊 Histogram Aggregation**: Accurate percentile calculation (<1% error) for distributed workloads
- **🗂️ Directory Tree Modes**: 3-mode system (Flat, DLIO Sharding, Hierarchical) for realistic dataset organization
- **🔍 Dry-Run Validation**: `--dry-run` flag validates configs and shows workload summary before execution
- **🌟 Multi-Process Scaling**: `--world-size N --rank R` distributed execution with shared memory coordination
- **🔥 Enterprise Coordination**: Atomic operations, cross-process barriers, zero temp files  
- **🚀 TRUE DLIO Parallel I/O**: Background workers with I/O+compute overlap for realistic performance
- **🎯 Complete Format Compatibility**: NPZ, HDF5, TFRecord validated with numpy, h5py, TensorFlow
- **🏪 Universal Storage**: File, S3/MinIO, Azure Blob, DirectIO backends with unified interface  
- **📋 DLIO Compatible**: Drop-in replacement for existing DLIO benchmark configurations
- **📊 Dual Metrics**: Separate storage (ops/s, MiB/s) and AI/ML (samples/s, batches/s) perspectives
- **☁️ Production Cloud Ready**: Real S3 and Azure credential support
- **🧪 Comprehensively Validated**: 119 comprehensive tests with golden reference validation and MLCommons DLIO compatibility

## 🧠 Workstream A: Realistic AI/ML Framework Simulation
```

### ✨ Key Features

- **� Distributed Controller**: Multi-agent orchestration with coordinated start and aggregate metrics
- **�🌟 Multi-Process Scaling**: `--world-size N --rank R` distributed execution with shared memory coordination
- **🔥 Enterprise Coordination**: Atomic operations, cross-process barriers, zero temp files  
- **🚀 TRUE DLIO Parallel I/O**: Background workers with I/O+compute overlap for realistic performance
- **🎯 Complete Format Compatibility**: NPZ, HDF5, TFRecord validated with numpy, h5py, TensorFlow
- **🏪 Universal Storage**: File, S3/MinIO, Azure Blob, DirectIO backends with unified interface  
- **📋 DLIO Compatible**: Drop-in replacement for existing DLIO benchmark configurations
- **📊 Dual Metrics**: Separate storage (ops/s, MiB/s) and AI/ML (samples/s, batches/s) perspectives
- **☁️ Production Cloud Ready**: Real S3 and Azure credential support
- **🧪 Comprehensively Validated**: 119 comprehensive tests with golden reference validation and MLCommons DLIO compatibility

## 🧠 Workstream A: Realistic AI/ML Framework Simulation

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

### Binaries
- **`dl-driver`**: Main CLI for single-process, multi-rank, and distributed controller execution
- **`dl_driver_agent`**: Standalone agent process for distributed workloads (gRPC service)

### Storage Backends
- **File System**: POSIX-compliant file I/O with DirectIO optimization
- **Cloud Storage**: S3/MinIO and Azure Blob with credential support
- **Performance**: Multi-GiB/s throughput with enterprise-grade reliability

### Data Formats  
- **NPZ, HDF5, TFRecord**: 100% compatible with numpy, h5py, and TensorFlow
- **Framework Support**: PyTorch, TensorFlow, and JAX configuration profiles
- **Validation**: Comprehensive test suite ensuring standard library compatibility

## 🏆 Key Achievements

### 🎯 Production-Ready AI/ML Data Pipeline
dl-driver has evolved into a complete, enterprise-grade testing framework for AI/ML workloads:

- **100% Format Compatibility**: All generated files work seamlessly with standard Python libraries (numpy, h5py, TensorFlow)
- **Distributed Orchestration**: Multi-agent coordination with histogram-based percentile aggregation (<1% error)
- **Results Directory**: Complete, reproducible results with per-agent and consolidated metrics in TSV format
- **DLIO Drop-in Replacement**: Full MLCommons configuration compatibility with enhanced features
- **Multi-Backend Excellence**: Unified performance across File, S3, Azure, and DirectIO storage
- **Enterprise Validation**: Comprehensive test suite ensuring reliability and correctness

### 📊 Validation Confidence
```
✅ Core Tests:       60/60 tests passing (metrics, config, workload, distributed, histogram aggregation)
✅ CLI Tests:        29/29 tests passing (configuration, backend integration)
✅ Integration Tests: 10/10 tests passing (histogram E2E, results directory workflow)
✅ Framework Tests:   7/7 tests passing (PyTorch integration, validation, serialization)
✅ Format Tests:      5/5 tests passing (NPZ, HDF5, TFRecord)
✅ Other Tests:       8/8 tests passing (replay, coordination, etc.)
✅ Total Coverage:  119/119 comprehensive tests validating all functionality
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

# Multi-rank execution (shared memory coordination)
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 0 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 1 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 2 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 3 &

# Distributed multi-agent execution
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 &
./target/release/dl_driver_agent --agent-id agent-1 --port 50052 &
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_local.yaml \
  --agents http://host1:50051,http://host2:50052 \
  --path-template "{id}/"

# Framework-specific workload profiles (Workstream A)
./target/release/dl-driver run --config config.yaml --profile torch
./target/release/dl-driver run --config config.yaml --profile tf
./target/release/dl-driver run --config config.yaml --profile jax

# Metrics export for CI/CD integration (Workstream A)
./target/release/dl-driver run --config config.yaml --metrics-json results.json
./target/release/dl-driver run --config config.yaml --metrics-csv results.csv

# Operation log validation (Workstream A)
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
dl-driver distributed --help       # Distributed multi-agent orchestration

# Multi-rank execution
dl-driver run --world-size N --rank R     # Multi-process shared memory coordination

# Distributed execution
dl_driver_agent --agent-id ID --port PORT  # Start agent process
dl-driver distributed run --agents LIST    # Controller for multi-agent workloads

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

### Validation Results
- ✅ **119 comprehensive tests** passing across all features
- ✅ **Format validation** with numpy, h5py, and TensorFlow standard libraries
- ✅ **Distributed workloads** validated with histogram aggregation and results directory output
- ✅ **Framework profiles** validated with PyTorch, TensorFlow, and JAX configurations
- ✅ **Operation log validation** tested with multi-million record production datasets
- ✅ **Metrics export** validated in JSON, CSV, and TSV formats for CI integration
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

