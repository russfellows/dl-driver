# dl-driver Quick Start Guide

**Get up and running with dl-driver in 10 minutes**

---

## 📋 What You'll Learn

- ✅ Install and build dl-driver
- ✅ Run your first workload
- ✅ Test with different storage backends
- ✅ Use checkpointing features
- ✅ Explore execution modes

**Current Version**: v0.8.4 (November 2025)  
**Prerequisites**: Rust 1.89.0+, Git, 15GB disk space

---

## 🚀 Installation (2 minutes)

### Clone and Build
### Clone and Build

```bash
# Clone repository
git clone https://github.com/russfellows/dl-driver.git
cd dl-driver

# Build release version
cargo build --release

# Verify installation
./target/release/dl-driver --version
# Output: dl-driver 0.8.4
```

**Build time**: ~3-5 minutes on modern hardware  
**Binary location**: `target/release/dl-driver`

---

## 🎯 Your First Workload (3 minutes)

### Single-Process Execution

Run a simple test with local filesystem storage:

```bash
# Create minimal config
cat > quick_test.yaml << 'EOF'
model: unet3d
dataset:
  num_files_train: 100
  num_samples_per_file: 1
  record_length: 1048576  # 1 MiB per sample
reader:
  data_loader: tensorflow
  batch_size: 4
  read_threads: 2
storage:
  storage_root: file:///tmp/dl_driver_quickstart
  storage_type: local_fs
workflow:
  generate_data: true
  train: true
train:
  epochs: 2
  computation_time: 0.001
EOF

# Run the workload
./target/release/dl-driver run --config quick_test.yaml

# Check results
ls /tmp/dl_driver_quickstart/
```

**What just happened?**
- Generated 100 synthetic NPZ files (1 MiB each)
- Simulated 2 epochs of training
- Produced metrics in TSV format
- Total time: ~10-30 seconds

---

## 🔧 Try Different Features (5 minutes)

### 1. Different Storage Backends

**Amazon S3 / MinIO:**
```bash
# Set credentials in .env file
cat > .env << 'EOF'
AWS_ACCESS_KEY_ID=your_key
AWS_SECRET_ACCESS_KEY=your_secret
AWS_REGION=us-east-1
AWS_ENDPOINT_URL=http://localhost:9000  # For MinIO
EOF

# Update config
storage:
  storage_root: s3://my-bucket/dl-driver-test
  storage_type: s3
```

**Google Cloud Storage:**
```bash
# Authenticate
gcloud auth application-default login

# Update config
storage:
  storage_root: gs://my-bucket/dl-driver-test
  storage_type: gcs
```

**Azure Blob Storage:**
```bash
# Set environment variables
export AZURE_STORAGE_ACCOUNT=myaccount
export AZURE_STORAGE_KEY=mykey

# Update config
storage:
  storage_root: az://myaccount/container/test
  storage_type: azure
```

### 2. Checkpoint Save & Reload

**Save checkpoints during training:**
```yaml
workflow:
  checkpoint: true

checkpoint:
  checkpoint_folder: file:///tmp/checkpoints
  checkpoint_after_epoch: 1
  epochs_between_checkpoints: 1
```

**Resume from checkpoint:**
```bash
# First run - saves checkpoints
./target/release/dl-driver run --config checkpoint_test.yaml

# List checkpoints
ls /tmp/checkpoints/

# Resume from checkpoint
./target/release/dl-driver run --config checkpoint_test.yaml \
  --resume-from-checkpoint file:///tmp/checkpoints/checkpoint_epoch_2_step_200.bin
```

See `examples/checkpoint_reload_example.yaml` for complete example.

### 3. Multi-Rank Execution (Shared Memory)

Simulate multi-GPU training on single node:

```bash
# Run with 4 ranks (simulates 4 GPUs)
./target/release/dl-driver run --config quick_test.yaml --num-ranks 4

# Each rank processes its own subset of data
# Shared memory coordination (no network overhead)
```

---

## � Understanding Output

After running a workload, you'll see:

```
/tmp/dl_driver_quickstart/
├── data/                          # Generated dataset files
│   ├── train_000000.npz
│   ├── train_000001.npz
│   └── ...
└── results/
    ├── storage_metrics_0.tsv      # Storage I/O metrics
    ├── ai_ml_metrics_0.tsv        # AI/ML framework metrics
    └── consolidated_report.txt    # Human-readable summary
```

**Key metrics:**
- **Storage metrics**: Throughput, latency, IOPS
- **AI/ML metrics**: Samples/sec, epoch time, batch processing
- **Percentiles**: p50, p95, p99 latencies

---

## 🎓 Next Steps

### Learn More Features

1. **Distributed Multi-Agent Execution**  
   Coordinate workloads across multiple hosts  
   → See [USER_GUIDE.md - Distributed Execution](USER_GUIDE.md#3-distributed-multi-agent-execution)

2. **Data Formats**  
   NPZ, HDF5, TFRecord with numpy/h5py/TensorFlow compatibility  
   → See [USER_GUIDE.md - Data Formats](USER_GUIDE.md#data-formats)

3. **DLIO Configuration**  
   Use existing DLIO benchmark configs  
   → See [USER_GUIDE.md - Configuration](USER_GUIDE.md#configuration)

4. **Advanced Checkpointing**  
   Multi-backend checkpoint save/reload  
   → See [USER_GUIDE.md - Checkpointing](USER_GUIDE.md#checkpointing)

### Example Configurations

Explore real-world examples in `tests/dlio_configs/`:
- `cosmoflow_config.yaml` - CosmoFlow (cosmology)
- `large_scale_threading_test.yaml` - High-throughput testing
- `cosmoflow_4hosts.yaml` - Distributed 4-node setup

### Full Documentation

- **[USER_GUIDE.md](USER_GUIDE.md)** - Complete feature documentation
- **[Changelog.md](Changelog.md)** - Version history and features
- **[RESULTS_DIRECTORY_FORMAT.md](RESULTS_DIRECTORY_FORMAT.md)** - Metrics specification
- **[DUAL_METRICS_REPORTING.md](DUAL_METRICS_REPORTING.md)** - Storage vs AI/ML metrics

---

## 🧪 Run Test Suite

Validate your installation:

```bash
# Run all tests (takes ~2 minutes)
cargo test --release

# Expected: 123/123 tests passing ✅

# Run specific test module
cargo test --release checkpoint

# Run with verbose output
cargo test --release -- --nocapture
```

---

## ⚡ Performance Tips

1. **Use `--release` builds** for realistic performance measurements
2. **Set appropriate `read_threads`** (typically 4-8 per process)
3. **Use `direct://` backend** for O_DIRECT file I/O (bypasses page cache)
4. **Enable checkpointing** to test realistic AI/ML workflows
5. **Monitor metrics** in results directory for bottleneck identification

---

## 🆘 Common Issues

### Build Errors

**Problem**: Compilation fails with dependency errors

**Solution**:
```bash
cargo clean
cargo update
cargo build --release
```

### Storage Authentication

**S3 not working**: Check `.env` file has credentials  
**GCS not working**: Run `gcloud auth application-default login`  
**Azure not working**: Set `AZURE_STORAGE_ACCOUNT` and `AZURE_STORAGE_KEY`

### Permission Errors

**Problem**: Can't write to storage location

**Solution**: Ensure directory exists and is writable:
```bash
mkdir -p /tmp/dl_driver_quickstart
chmod 755 /tmp/dl_driver_quickstart
```

---

## 📚 Additional Resources

- **GitHub Repository**: https://github.com/russfellows/dl-driver
- **s3dlio Library**: https://github.com/russfellows/s3dlio (storage engine)
- **Issues/Questions**: Open GitHub issue
- **License**: GPL v3

---

## ✅ Success Checklist

After completing this guide, you should have:

- ✅ Built dl-driver from source
- ✅ Run a simple workload successfully
- ✅ Generated synthetic dataset and metrics
- ✅ Understood basic configuration options
- ✅ Explored checkpoint features
- ✅ Know where to find detailed documentation

**Ready for production testing?** → Read [USER_GUIDE.md](USER_GUIDE.md) for complete documentation.

---

**Quick Start Version**: 2.0  
**Last Updated**: November 3, 2025  
**dl-driver Version**: 0.8.4