# dl-driver User Guide

**Version:** 0.8.2  
**Last Updated:** November 2, 2025

## Table of Contents

1. [Introduction](#introduction)
2. [Installation](#installation)
3. [Execution Modes](#execution-modes)
4. [Configuration](#configuration)
5. [Storage Backends](#storage-backends)
6. [Data Formats](#data-formats)
7. [Metrics and Reporting](#metrics-and-reporting)
8. [Advanced Usage](#advanced-usage)
9. [Troubleshooting](#troubleshooting)

---

## Introduction

**dl-driver** is a high-performance tool for testing storage performance during AI/ML workloads. It provides format compatibility with standard Python libraries (numpy, h5py, TensorFlow) and serves as a drop-in replacement for MLCommons DLIO benchmarks.

### Key Capabilities

- **Three Execution Modes**: Single-process, multi-rank (shared memory), and distributed (multi-agent)
- **Universal Storage**: File, S3/MinIO, Azure Blob, DirectIO backends
- **Format Validation**: NPZ, HDF5, TFRecord with 100% Python library compatibility
- **Dual Metrics**: Separate perspectives for storage engineers (ops/s, MiB/s) and ML engineers (samples/s, batches/s)
- **DLIO Compatible**: Uses MLCommons DLIO YAML configuration format

---

## Installation

### Prerequisites

- **Rust**: 1.89.0 or later
- **Python** (optional): For format validation
- **Cloud Credentials** (optional): For S3/Azure testing

### Build from Source

```bash
git clone https://github.com/russfellows/dl-driver.git
cd dl-driver
cargo build --release
```

Binaries will be available in `./target/release/`:
- `dl-driver`: Main CLI tool
- `dl_driver_agent`: Distributed agent service

### Verify Installation

```bash
./target/release/dl-driver --version
./target/release/dl_driver_agent --version
```

---

## Execution Modes

### 1. Single-Process Execution

**Use Case**: Basic testing, development, single-node workloads

```bash
# Run a complete DLIO workload (generate + train)
./target/release/dl-driver run --config tests/dlio_configs/minimal_config.yaml

# Validate configuration before execution (dry-run)
./target/release/dl-driver run --config tests/dlio_configs/minimal_config.yaml --dry-run

# Generate data only
./target/release/dl-driver generate --config tests/dlio_configs/minimal_config.yaml

# Validate configuration without running
./target/release/dl-driver validate --config tests/dlio_configs/minimal_config.yaml
```

**Configuration Validation (--dry-run):**

The `--dry-run` flag validates your configuration and shows a detailed workload summary without executing:

```bash
./target/release/dl-driver run --config myconfig.yaml --dry-run
```

Output includes:
- ✅ Model configuration
- ✅ Workflow phases enabled
- ✅ Backend detection (file vs object store)
- ✅ Directory structure analysis
- ✅ Training workload estimation (batches, total I/O, AU calculation)

See [`docs/DRY_RUN_FEATURE.md`](DRY_RUN_FEATURE.md) for complete details.

**Example Output:**
```
✅ Workload completed successfully!
📊 Files processed: 20
📊 Data read: 20.0 MiB
📊 Throughput: 150.2 MiB/s
📊 Runtime: 0.133s
```

### 2. Multi-Rank Execution (Shared Memory)

**Use Case**: Multi-GPU simulation on single node, shared memory coordination

```bash
# Launch 4 processes simulating 4 GPUs
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 0 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 1 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 2 &
./target/release/dl-driver run --config config.yaml --world-size 4 --rank 3 &
```

**Features:**
- Zero temp files (atomic shared memory operations)
- Automatic aggregation on rank 0
- Synchronized start/stop timing
- Interleaved file sharding

**Example Output (Rank 0):**
```
🎉 Multi-Rank Results (Shared Memory Coordination):
================================================================
Total files processed: 80
Total data read: 1.2 GiB
Combined throughput: 4.5 GiB/s
Global runtime: 0.267s
Number of ranks: 4
✅ Multi-rank coordination successful - NO TEMP FILES USED
```

### 3. Distributed Multi-Agent Execution

**Use Case**: True multi-host workloads, enterprise-scale testing

#### Step 1: Start Agent Processes

On each host, start the agent service:

```bash
# Host 1
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 --bind-addr 0.0.0.0

# Host 2
./target/release/dl_driver_agent --agent-id agent-1 --port 50051 --bind-addr 0.0.0.0

# For verbose logging, add -v or -vv
./target/release/dl_driver_agent --agent-id agent-0 --port 50051 -vv
```

#### Step 2: Run Controller

From any host, run the controller:

```bash
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_local.yaml \
  --agents http://host1:50051,http://host2:50051 \
  --path-template "{id}/"
```

**Path Template Options:**
- `--path-template "{id}/"`: Agent-specific subdirectories (local storage)
- Omit for shared storage (GCS, S3, Azure)

**Example Output:**
```
╔════════════════════════════════════════════════╗
║   Distributed Workload Complete! 🎉           ║
╚════════════════════════════════════════════════╝

📊 Storage Performance (I/O Perspective):
   Total Throughput: 687.5 MiB/s
   Total Operations: 40
   Average Latency: p50=0.00ms, p90=0.00ms, p95=0.00ms, p99=0.00ms
   Errors: 0

🤖 AI/ML Training Performance (Training Perspective):
   Training Velocity: 297.9 samples/s, 45.8 batches/s
   Total Samples: 40, Total Batches: 8
   Average Batch Time: 21.83ms
   Epochs Completed: 4
   Pipeline Efficiency: 37.8%
```

---

## Configuration

### DLIO Configuration Format

dl-driver uses YAML configuration compatible with MLCommons DLIO:

```yaml
model:
  name: my_workload

framework: pytorch  # pytorch, tensorflow, or None

workflow:
  generate_data: true
  train: true
  checkpoint: false

dataset:
  data_folder: file:///data/training  # See Storage Backends
  format: npz                         # npz, hdf5, tfrecord
  num_files_train: 100
  record_length_bytes: 1048576        # 1MB per record
  
  # Directory organization (optional - see Directory Modes below)
  # num_subfolders_train: 8           # Mode 2: DLIO sharding
  # directory_tree:                   # Mode 3: Hierarchical
  #   width: 4
  #   depth: 2
  #   files_per_dir: 16

reader:
  data_loader: pytorch
  batch_size: 32
  read_threads: 4
  compute_threads: 2
  prefetch: 8
  shuffle: true

train:
  epochs: 5
  computation_time: 0.05  # 50ms simulated compute per batch
  seed: 42
```

### Directory Organization Modes

dl-driver supports 3 directory organization modes for realistic dataset structures:

**Mode 1: Flat (Default)**
```yaml
dataset:
  num_files_train: 1000
  # No directory configuration = flat mode (all files in single directory)
```

**Mode 2: DLIO-Style Sharding**
```yaml
dataset:
  num_files_train: 10000
  num_subfolders_train: 32  # Creates train/0000 through train/0031
```

**Mode 3: Hierarchical Tree**
```yaml
dataset:
  directory_tree:
    width: 32          # 32 branches at each level
    depth: 2           # 2 levels deep
    files_per_dir: 100 # 100 files per leaf directory
    # Total: 32×32 = 1,024 directories, 102,400 files
```

**Documentation:** See [`tests/dlio_configs/DIRECTORY_MODES_README.md`](../tests/dlio_configs/DIRECTORY_MODES_README.md) for complete guide with decision tree, performance considerations, and full-scale examples.

### Configuration Examples

Pre-configured examples available in `tests/dlio_configs/`:

- **`minimal_config.yaml`**: Simple 20-file test
- **`unet3d_config.yaml`**: MLCommons UNet3D workload
- **`bert_config.yaml`**: MLCommons BERT workload
- **`distributed_2node_local.yaml`**: 2-node distributed with local storage
- **`distributed_2node_gcs.yaml`**: 2-node distributed with GCS
- **`distributed_4node_local.yaml`**: 4-node distributed with checkpointing

---

## Storage Backends

### Local Filesystem (`file://`)

```yaml
dataset:
  data_folder: file:///mnt/data/training
```

**Features:**
- Standard POSIX file I/O
- DirectIO optimization available (`direct://`)
- Path isolation for distributed agents

**Distributed Usage:**
- Requires `--path-template "{id}/"` for agent isolation
- Creates subdirectories: `/mnt/data/training/agent-0/`, `/mnt/data/training/agent-1/`, etc.

### DirectIO (`direct://`)

```yaml
dataset:
  data_folder: direct:///mnt/nvme/data
```

**Features:**
- Bypasses OS page cache (O_DIRECT)
- Optimal for high-speed storage (NVMe, parallel filesystem)
- Aligned I/O required

### Amazon S3 / MinIO (`s3://`)

```yaml
dataset:
  data_folder: s3://my-bucket/training-data
```

**Setup:**
```bash
# Set credentials in .env file
echo "AWS_ACCESS_KEY_ID=your_key" >> .env
echo "AWS_SECRET_ACCESS_KEY=your_secret" >> .env
echo "AWS_REGION=us-east-1" >> .env
```

**Distributed Usage:**
- Shared storage - no path template needed
- All agents read/write to same bucket/prefix

### Google Cloud Storage (`gs://`)

```yaml
dataset:
  data_folder: gs://my-bucket/training-data
```

**Setup:**
```bash
# Authenticate with application default credentials
gcloud auth application-default login
```

**Distributed Usage:**
- Shared storage - no path template needed
- All agents access same bucket

### Azure Blob Storage (`az://`)

```yaml
dataset:
  data_folder: az://container/training-data
```

**Setup:**
```bash
# Set environment variables
export AZURE_STORAGE_ACCOUNT=myaccount
export AZURE_STORAGE_CONTAINER=mycontainer
export AZURE_STORAGE_KEY=your_key
```

**Distributed Usage:**
- Shared storage - no path template needed

---

## Data Formats

### NPZ (NumPy Archive)

```yaml
dataset:
  format: npz
  record_length_bytes: 1048576  # Size of numpy array in bytes
```

**Validation:**
```python
import numpy as np
data = np.load('train_file_000000.npz')
print(data['records'].shape)  # (n_samples,)
```

**Use Case**: General-purpose, excellent Python compatibility

### HDF5

```yaml
dataset:
  format: hdf5
  record_length_bytes: 2097152  # 2MB per record
```

**Validation:**
```python
import h5py
with h5py.File('train_file_000000.h5', 'r') as f:
    data = f['records'][:]
    print(data.shape)
```

**Use Case**: Large datasets, hierarchical data

### TFRecord

```yaml
dataset:
  format: tfrecord
  record_length_bytes: 524288  # 512KB per record
```

**Validation:**
```python
import tensorflow as tf
dataset = tf.data.TFRecordDataset('train_file_000000.tfrecord')
for record in dataset.take(1):
    print(tf.io.parse_tensor(record, out_type=tf.uint8).shape)
```

**Use Case**: TensorFlow pipelines, streaming

---

## Metrics and Reporting

### Dual Metrics System

dl-driver provides two perspectives on performance:

#### 1. Storage Metrics (I/O Perspective)

**For Storage Engineers:**
```
📊 Storage Performance:
   Total Throughput: 2044.2 MiB/s
   Total Operations: 196
   Average Latency: p50=0.00ms, p90=0.00ms, p95=0.01ms, p99=0.02ms
   Errors: 0
```

**TSV Output:** `workload_storage_metrics.tsv`
- ops/s, MiB/s, bytes_processed
- Latency percentiles (p50/p90/p95/p99)
- Error counts

#### 2. AI/ML Training Metrics (Training Perspective)

**For ML Engineers:**
```
🤖 AI/ML Training Performance:
   Training Velocity: 313.0 samples/s, 25.6 batches/s
   Total Samples: 196, Total Batches: 16
   Average Batch Time: 21.10ms
   Epochs Completed: 12
   Pipeline Efficiency: 30.3%
```

**TSV Output:** `workload_aiml_metrics.tsv`
- samples/s, batches/s
- Epoch timing, batch timing
- Pipeline efficiency (I/O vs compute overlap)

### Metric Files

After execution, find metrics in:
- `distributed_storage_metrics.tsv` (storage perspective)
- `distributed_aiml_metrics.tsv` (AI/ML perspective)

Import into Excel, pandas, or your analysis tool of choice.

---

## Advanced Usage

### MLPerf Compliance Mode

Enhanced reporting for MLPerf-style benchmarks:

```bash
./target/release/dl-driver run --mlperf --config config.yaml --format json
```

### Framework-Specific Profiles

Optimize for specific ML frameworks:

```bash
# PyTorch-optimized
./target/release/dl-driver run --profile torch --config config.yaml

# TensorFlow-optimized
./target/release/dl-driver run --profile tf --config config.yaml

# JAX-optimized
./target/release/dl-driver run --profile jax --config config.yaml
```

### Custom Metrics Export

```bash
# Export to JSON
./target/release/dl-driver run --config config.yaml --metrics-json results.json

# Export to CSV
./target/release/dl-driver run --config config.yaml --metrics-csv results.csv
```

### Checkpointing

dl-driver supports **saving** and **reloading** checkpoints across all storage backends (file://, direct://, s3://, az://, gs://).

#### Saving Checkpoints

Enable periodic checkpoints during training:

```yaml
checkpoint:
  checkpoint_folder: file:///checkpoints  # Any backend supported
  checkpoint_after_epoch: 1               # Start checkpointing after epoch 1
  epochs_between_checkpoints: 2           # Save every 2 epochs
  steps_between_checkpoints: 100          # Save every 100 steps
```

**Multi-Backend Examples:**
```yaml
# Local filesystem
checkpoint_folder: file:///mnt/checkpoints

# Direct I/O
checkpoint_folder: direct:///nvme/checkpoints

# Amazon S3
checkpoint_folder: s3://my-bucket/training-run-001/checkpoints

# Google Cloud Storage
checkpoint_folder: gs://my-bucket/ml-checkpoints

# Azure Blob Storage
checkpoint_folder: az://myaccount/container/checkpoints
```

#### Reloading Checkpoints (v0.8.4+)

Resume training from a saved checkpoint:

```bash
# Resume from local checkpoint
dl-driver run --config config.yaml --resume-from-checkpoint file:///checkpoints/checkpoint_epoch_5_step_500.bin

# Resume from S3
dl-driver run --config config.yaml --resume-from-checkpoint s3://bucket/checkpoints/checkpoint_epoch_3.bin

# Resume from GCS  
dl-driver run --config config.yaml --resume-from-checkpoint gs://bucket/ckpt/checkpoint_epoch_10.bin

# Resume from Azure
dl-driver run --config config.yaml --resume-from-checkpoint az://account/container/checkpoint_epoch_2.bin
```

**Resume Behavior:**
- Training resumes at the **start of the next epoch** after the checkpoint
- Example: Checkpoint saved at epoch 5, step 500 → resumes at epoch 6, step 0
- Avoids mid-epoch complexities and ensures clean batch boundaries
- All checkpoint metadata is validated on load

**Testing Examples:**
- See `crates/cli/tests/checkpoint_multibackend_test.rs` for 5-backend integration tests
- See `crates/cli/tests/checkpoint_scenarios_test.rs` for 4 comprehensive reload scenarios
- See `tests/manual_checkpoint_test.sh` for real-world validation script

---

## Troubleshooting

### Agent Connection Issues

**Problem:** Controller can't connect to agents

**Solutions:**
1. Check agents are running: `ps aux | grep dl_driver_agent`
2. Verify network connectivity: `curl http://host:50051`
3. Check firewall rules allow gRPC port (50051)
4. Use `-vv` on agents for verbose logging

### Path Template Errors

**Problem:** "Failed to write file" errors in distributed mode

**Solutions:**
1. For **local storage** (`file://`), use `--path-template "{id}/"`
2. For **shared storage** (`gs://`, `s3://`, `az://`), omit path template
3. Verify base directory exists and is writable
4. Check agent has permissions

### Storage Authentication

**S3 Issues:**
```bash
# Check .env file
cat .env
# Should contain: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION
```

**GCS Issues:**
```bash
# Re-authenticate
gcloud auth application-default login
# Verify credentials
gcloud auth application-default print-access-token
```

**Azure Issues:**
```bash
# Check environment variables
env | grep AZURE
# Should show: AZURE_STORAGE_ACCOUNT, AZURE_STORAGE_KEY
```

### Performance Issues

**Low throughput:**
1. Increase `read_threads` in config
2. Enable `prefetch` in reader section
3. Use DirectIO for local NVMe storage
4. Check network bandwidth for cloud storage

**High latency:**
1. Reduce `batch_size` for faster iteration
2. Increase `compute_threads` for better overlap
3. Enable shuffle for realistic access patterns

### Multi-Rank Coordination

**Problem:** Processes hang at startup

**Solutions:**
1. Ensure all ranks launched with same `--world-size`
2. Check shared memory permissions: `/dev/shm`
3. Clean up stale shared memory: `ls /dev/shm/dl_driver_*`
4. Use unique config for each test to avoid conflicts

---

## Additional Resources

- **Quick Start Guide**: `docs/QUICK_START.md`
- **Distributed Setup**: `tests/dlio_configs/DISTRIBUTED_README.md`
- **Changelog**: `docs/Changelog.md`
- **Dual Metrics Spec**: `docs/DUAL_METRICS_REPORTING.md`

## Support

- **Issues**: https://github.com/russfellows/dl-driver/issues
- **Documentation**: https://github.com/russfellows/dl-driver/tree/main/docs
- **License**: GPL v3.0 (see LICENSE)
