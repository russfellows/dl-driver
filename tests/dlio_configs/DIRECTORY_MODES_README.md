# Directory Structure Mode Configurations

This directory contains example configurations demonstrating dl-driver's three directory organization modes for dataset generation and training.

## Overview

dl-driver supports three directory structure modes to match different storage testing scenarios and real-world ML dataset layouts:

1. **Mode 1 (Flat)** - All files in single directory (simplest, high metadata load)
2. **Mode 2 (DLIO Sharding)** - Files distributed across flat subdirectories (DLIO-compatible)
3. **Mode 3 (Hierarchical)** - Nested tree structure (realistic ML datasets like ImageNet)

## Configuration Files

### Small Test Configs (~2.5 GB each)
Quick verification tests that complete in minutes:

- **`test_mode1_small_flat.yaml`** - 256 files × 10 MB, single directory
- **`test_mode2_small_sharding.yaml`** - 256 files × 10 MB, 8 subdirectories
- **`test_mode3_small_hierarchical.yaml`** - 256 files × 10 MB, 4×4 tree (20 dirs)

### Full-Scale Configs (~100 GB each)
Production-scale tests for realistic benchmarking:

- **`resnet50_1host_mode1_flat.yaml`** - 102,400 files × 1 MB, single directory
- **`resnet50_1host_mode2_sharding.yaml`** - 102,400 files × 1 MB, 32 subdirectories
- **`resnet50_1host_mode3_hierarchical.yaml`** - 102,400 files × 1 MB, 32×2 tree (1,056 dirs)

## Quick Start

### 1. Validate Configuration (--dry-run)

Before running, use `--dry-run` to validate config and preview test parameters:

```bash
cd /home/eval/Documents/Code/dl-driver

# Validate Mode 1 (Flat)
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml --dry-run

# Validate Mode 2 (DLIO Sharding)
./target/release/dl-driver run --config tests/dlio_configs/test_mode2_small_sharding.yaml --dry-run

# Validate Mode 3 (Hierarchical)
./target/release/dl-driver run --config tests/dlio_configs/test_mode3_small_hierarchical.yaml --dry-run
```

The dry-run output shows:
- Directory structure mode and layout
- Total files and directories
- Storage requirements
- Training workload estimation

### 2. Run Small Tests

Test all three modes with small datasets (~2.5 GB each):

```bash
# Clean test directory
rm -rf /tmp/dldriver_test

# Run Mode 1 (Flat)
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml

# Run Mode 2 (DLIO Sharding)
./target/release/dl-driver run --config tests/dlio_configs/test_mode2_small_sharding.yaml

# Run Mode 3 (Hierarchical)
./target/release/dl-driver run --config tests/dlio_configs/test_mode3_small_hierarchical.yaml
```

### 3. Verify Directory Structure

#### Mode 1 (Flat)
```bash
# Should see 256 files in single directory
find /tmp/dldriver_test/mode1_flat -name "*.npz" | wc -l
# Expected: 256

ls /tmp/dldriver_test/mode1_flat/train_file_* | head -5
# Expected:
#   train_file_00000000.npz
#   train_file_00000001.npz
#   train_file_00000002.npz
#   ...
```

#### Mode 2 (DLIO Sharding)
```bash
# Should see 8 subdirectories
ls -d /tmp/dldriver_test/mode2_sharding/train/????
# Expected:
#   train/0000 train/0001 train/0002 train/0003
#   train/0004 train/0005 train/0006 train/0007

# Should see 32 files per subdirectory
ls /tmp/dldriver_test/mode2_sharding/train/0000/train_file_* | wc -l
# Expected: 32

ls /tmp/dldriver_test/mode2_sharding/train/0000/train_file_* | head -3
# Expected:
#   train/0000/train_file_00000000.npz  (0 % 8 = 0)
#   train/0000/train_file_00000008.npz  (8 % 8 = 0)
#   train/0000/train_file_00000016.npz  (16 % 8 = 0)
```

#### Mode 3 (Hierarchical)
```bash
# Should see 20 total directories (4 L1 + 16 L2)
find /tmp/dldriver_test/mode3_hierarchical -name "*.dir" -type d | wc -l
# Expected: 20

# List level 1 directories
ls -d /tmp/dldriver_test/mode3_hierarchical/test.d1_w*.dir
# Expected:
#   test.d1_w0.dir test.d1_w1.dir test.d1_w2.dir test.d1_w3.dir

# List level 2 directories under one L1
ls -d /tmp/dldriver_test/mode3_hierarchical/test.d1_w0.dir/test.d2_w*.dir
# Expected:
#   test.d2_w0.dir test.d2_w1.dir test.d2_w2.dir test.d2_w3.dir

# Should see 16 files per leaf directory
ls /tmp/dldriver_test/mode3_hierarchical/test.d1_w0.dir/test.d2_w0.dir/train_file_* | wc -l
# Expected: 16
```

## Directory Structure Modes Explained

### Mode 1: Flat Directory

**Config Example:**
```yaml
dataset:
  num_files_train: 256
  # No directory structure fields = Mode 1 (Flat)
```

**Result:**
- All files in single directory
- Simple to manage, but high metadata load
- File naming: `train_file_00000000.npz`, `train_file_00000001.npz`, ...

**Use Cases:**
- Baseline testing
- Small datasets (<1K files)
- Systems with optimized single-directory handling

### Mode 2: DLIO Sharding

**Config Example:**
```yaml
dataset:
  num_files_train: 256
  num_subfolders_train: 8  # Creates train/0000 through train/0007
```

**Result:**
- Files distributed across flat subdirectories
- Distribution: `file_i → train/{i % num_subfolders}`
- DLIO-compatible layout
- File naming: `train/0000/train_file_00000000.npz`, `train/0001/train_file_00000001.npz`

**Use Cases:**
- DLIO benchmark compatibility
- Medium datasets (1K-100K files)
- Balanced metadata load across subdirectories

### Mode 3: Hierarchical Tree

**Config Example:**
```yaml
dataset:
  directory_tree:
    width: 4              # 4 subdirectories per level
    depth: 2              # 2 levels deep
    files_per_dir: 16     # 16 files per leaf directory
    distribution: bottom  # Files only in leaf directories
    dir_mask: "test.d%d_w%d.dir"
```

**Result:**
- Nested tree structure: `width^depth` leaf directories
- Total dirs: `width + width^depth` (e.g., 4 + 16 = 20)
- Total files: `width^depth × files_per_dir` (e.g., 16 × 16 = 256)
- File naming: `test.d1_w0.dir/test.d2_w0.dir/train_file_00000000.npz`

**Use Cases:**
- Realistic ML datasets (ImageNet, COCO)
- Large-scale storage testing (100K+ files)
- Filesystem metadata scalability testing
- sai3-bench apples-to-apples comparison

## Object Store Compatibility

All three modes work seamlessly with object stores (S3, Azure Blob, GCS):

### Local Filesystem (`file://` or `direct://`)
- Explicit directories created with `mkdir()`
- Verify with: `find /path -type d`

### Object Stores (`s3://`, `az://`, `gs://`)
- Directories are implicit (no explicit `mkdir()`)
- Directory structure reflected in object key paths
- Example S3 keys:
  - Mode 1: `s3://bucket/train_file_00000000.npz`
  - Mode 2: `s3://bucket/train/0000/train_file_00000000.npz`
  - Mode 3: `s3://bucket/test.d1_w0.dir/test.d2_w0.dir/train_file_00000000.npz`

**To test with S3:**
```yaml
dataset:
  data_folder: s3://your-bucket/dl-driver-test
  # ... rest of config unchanged
```

## Performance Considerations

### Metadata Operations

| Mode | Directories | Files/Dir | Metadata Load |
|------|-------------|-----------|---------------|
| Mode 1 (Flat) | 1 | 102,400 | Very High |
| Mode 2 (DLIO) | 32 | 3,200 | Medium |
| Mode 3 (Hierarchical) | 1,056 | 100 | Low |

**Recommendation:** Use Mode 3 for large-scale testing (>10K files) to minimize metadata bottlenecks.

### Storage System Impact

- **Parallel Filesystems (Lustre, GPFS):** Mode 2 or 3 recommended for better metadata distribution
- **Object Stores (S3, GCS):** Mode 1 acceptable (no metadata servers)
- **Local SSDs:** Mode 1 or 2 sufficient (<10K files)

## Customization

### Adjust File Size
```yaml
dataset:
  record_length_bytes: 10485760  # 10 MB per file
  num_samples_per_file: 1        # 1 sample per file
```

### Adjust Mode 2 Sharding
```yaml
dataset:
  num_subfolders_train: 64  # More subdirectories = fewer files per subdir
```

### Adjust Mode 3 Tree Depth
```yaml
dataset:
  directory_tree:
    width: 8    # 8 subdirs per level
    depth: 3    # 3 levels deep = 8³ = 512 leaf dirs
    files_per_dir: 200
    # Total files = 512 × 200 = 102,400
```

### Distribution Options (Mode 3)
```yaml
dataset:
  directory_tree:
    distribution: bottom  # Files only in leaf directories (default)
    # OR
    distribution: all     # Files at every level (increases total files)
```

## Troubleshooting

### Mode 2: Files not distributed correctly
- Verify `num_subfolders_train` is set
- Check modulo distribution: `file_index % num_subfolders_train`
- Example: File 42 with 8 subdirs → `42 % 8 = 2` → `train/0002/`

### Mode 3: Wrong number of directories
- Check calculation: `total_dirs = width + width^depth`
  - Example: `width=4, depth=2` → `4 + 4² = 4 + 16 = 20`
- Verify depth matches config
- Use `find -type d | wc -l` to count

### Mode 3: Wrong number of files
- Check calculation: `total_files = width^depth × files_per_dir`
  - Example: `width=4, depth=2, files_per_dir=16` → `16 × 16 = 256`
- Verify distribution mode (`bottom` vs `all`)

### Object store: Directories not created
- **This is expected!** Object stores don't have directories
- Directory structure is reflected in object key paths
- Verify with: `aws s3 ls s3://bucket/ --recursive`

## Next Steps

1. **Start with small tests** - Validate functionality with ~2.5 GB configs
2. **Run full-scale tests** - Benchmark with ~100 GB configs
3. **Compare modes** - Measure performance differences
4. **Test object stores** - Verify S3/Azure/GCS behavior
5. **Customize configs** - Adjust for your storage system

## Related Documentation

- `DRY_RUN_FEATURE.md` - Details on --dry-run flag
- `DISTRIBUTED_README.md` - Multi-host distributed execution
- Main project `README.md` - General dl-driver usage
