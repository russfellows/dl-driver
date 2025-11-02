# dl-driver --dry-run Feature & Small Test Configs

## Summary

Successfully added `--dry-run` CLI flag to dl-driver (similar to sai3-bench) for configuration validation before execution. This allows users to:
1. **Validate YAML syntax** - Ensure config parses correctly
2. **Preview test parameters** - See what will actually run
3. **Estimate resources** - Calculate storage requirements, training time, directory structure
4. **Catch configuration errors early** - Before consuming storage/compute

## Usage

```bash
# Validate configuration without running
dl-driver run --config my_config.yaml --dry-run

# Run after validation
dl-driver run --config my_config.yaml
```

## Implementation

### CLI Changes
- Added `--dry-run` flag to `Run` command in `crates/cli/src/main.rs`
- Flag checked immediately after config loading, before execution
- Calls `display_config_summary()` and exits

### Display Output
The `display_config_summary()` function shows:

1. **Model Configuration** - Name, size, framework
2. **Workflow Phases** - Which phases are enabled (generate_data, train, checkpoint, evaluation)
3. **Dataset Configuration** - Data folder, backend type, format, record size, samples/file
4. **Directory Structure** - Mode-specific analysis:
   - **Mode 1 (Flat)**: Single directory, file count, total size
   - **Mode 2 (DLIO Sharding)**: Number of subdirectories, distribution pattern, files per subdir
   - **Mode 3 (Hierarchical)**: Width, depth, tree metrics, nested structure
5. **Data Loader Configuration** - Loader type, batch size, threads, prefetch, transfer size, shuffle
6. **Training Configuration** - Epochs, computation time, estimated workload (samples, batches, compute time)
7. **Checkpoint Configuration** - Folder, intervals
8. **Object Store Warnings** - Notes for S3/Azure/GCS (implicit directories)

### Example Output

```
╔═══════════════════════════════════════════════════════════════════════╗
║         DL-DRIVER CONFIGURATION VALIDATION & TEST SUMMARY             ║
╚═══════════════════════════════════════════════════════════════════════╝

✅ Config file parsed successfully: /path/to/config.yaml

┌─ Model Configuration ────────────────────────────────────────────────┐
│ Model Name:   test_flat_mode
│ Model Size:   1000000 parameters
└──────────────────────────────────────────────────────────────────────┘

Framework: pytorch

┌─ Workflow Phases ────────────────────────────────────────────────────┐
│ Generate Data:  ✅ YES
│ Training:       ✅ YES
│ Checkpoint:     ❌ NO
│ Evaluation:     ❌ NO
└──────────────────────────────────────────────────────────────────────┘

┌─ Dataset Configuration ──────────────────────────────────────────────┐
│ Data Folder:  file:///tmp/dldriver_test/mode1_flat
│ Backend Type: Local Filesystem (file://)
│ Format:       npz
│ Record Size:  10485760 bytes (10.00 MB)
│ Samples/File: 1
└──────────────────────────────────────────────────────────────────────┘

┌─ Directory Structure: Mode 1 (Flat) ────────────────────────────────┐
│ Structure:     Single directory (all files in one location)
│ Files:         256 training files
│ Path Pattern:  train_file_{:08}.npz
│ Example:       train_file_00000000.npz
│                train_file_00000001.npz
│ Total Size:    2.50 GB
└──────────────────────────────────────────────────────────────────────┘

... (additional sections) ...

╔═══════════════════════════════════════════════════════════════════════╗
║                         DRY-RUN VALIDATION COMPLETE                   ║
╚═══════════════════════════════════════════════════════════════════════╝

✅ Configuration is valid and ready to execute.
   Remove --dry-run flag to run the workload.
```

## Small Test Configurations

Created 3 small test configs (~2.5 GB each) for quick verification:

### 1. Mode 1 (Flat) - `test_mode1_small_flat.yaml`
- **Size**: 256 files × 10 MB = 2.56 GB
- **Structure**: Single directory
- **Location**: `/tmp/dldriver_test/mode1_flat`
- **Training**: 2 epochs, 32 batches
- **Example**: `train_file_00000000.npz`, `train_file_00000001.npz`

### 2. Mode 2 (DLIO Sharding) - `test_mode2_small_sharding.yaml`
- **Size**: 256 files × 10 MB = 2.56 GB
- **Structure**: 8 flat subdirectories (32 files each)
- **Location**: `/tmp/dldriver_test/mode2_sharding`
- **Distribution**: Modulo sharding (file_i → train/{i % 8})
- **Training**: 2 epochs, 32 batches
- **Example**: `train/0000/train_file_00000000.npz`, `train/0001/train_file_00000001.npz`

### 3. Mode 3 (Hierarchical) - `test_mode3_small_hierarchical.yaml`
- **Size**: 256 files × 10 MB = 2.56 GB
- **Structure**: 4×4 nested tree (20 directories total, 16 leaf dirs)
- **Location**: `/tmp/dldriver_test/mode3_hierarchical`
- **Tree**: width=4, depth=2, files_per_dir=16
- **Training**: 2 epochs, 32 batches
- **Example**: `test.d1_w0.dir/test.d2_w0.dir/train_file_00000000.npz`

## Test Workflow

### 1. Validate All Configs
```bash
cd /home/eval/Documents/Code/dl-driver

# Test Mode 1 (Flat)
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml --dry-run

# Test Mode 2 (DLIO Sharding)
./target/release/dl-driver run --config tests/dlio_configs/test_mode2_small_sharding.yaml --dry-run

# Test Mode 3 (Hierarchical)
./target/release/dl-driver run --config tests/dlio_configs/test_mode3_small_hierarchical.yaml --dry-run
```

### 2. Run Small Tests
```bash
# Clean test directory
rm -rf /tmp/dldriver_test

# Run Mode 1 (Flat) - should create 256 files in single directory
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml

# Verify structure
ls -lh /tmp/dldriver_test/mode1_flat/train_file_* | head -10
du -sh /tmp/dldriver_test/mode1_flat

# Run Mode 2 (DLIO Sharding) - should create 8 subdirectories
./target/release/dl-driver run --config tests/dlio_configs/test_mode2_small_sharding.yaml

# Verify structure
ls -d /tmp/dldriver_test/mode2_sharding/train/????
ls -lh /tmp/dldriver_test/mode2_sharding/train/0000/train_file_* | head -5
du -sh /tmp/dldriver_test/mode2_sharding

# Run Mode 3 (Hierarchical) - should create nested tree
./target/release/dl-driver run --config tests/dlio_configs/test_mode3_small_hierarchical.yaml

# Verify structure
find /tmp/dldriver_test/mode3_hierarchical -name "*.dir" -type d | head -10
ls -lh /tmp/dldriver_test/mode3_hierarchical/test.d1_w0.dir/test.d2_w0.dir/train_file_* | head -5
du -sh /tmp/dldriver_test/mode3_hierarchical
```

### 3. Verify Directory Counts
```bash
# Mode 1: Should be 1 directory (train root)
find /tmp/dldriver_test/mode1_flat -type d | wc -l

# Mode 2: Should be 9 directories (1 train + 8 subdirs)
find /tmp/dldriver_test/mode2_sharding -type d | wc -l

# Mode 3: Should be 20 directories (4 L1 + 16 L2)
find /tmp/dldriver_test/mode3_hierarchical -name "*.dir" -type d | wc -l
```

### 4. Verify File Counts
```bash
# All modes: Should have 256 NPZ files
find /tmp/dldriver_test/mode1_flat -name "*.npz" | wc -l
find /tmp/dldriver_test/mode2_sharding -name "*.npz" | wc -l
find /tmp/dldriver_test/mode3_hierarchical -name "*.npz" | wc -l
```

## Benefits

### For Users
- **No surprises** - Know exactly what will happen before execution
- **Storage planning** - Calculate space requirements upfront
- **Config debugging** - Fix YAML errors without consuming resources
- **Time estimation** - See estimated training time

### For Development
- **Faster iteration** - Test config changes without full runs
- **Catch bugs early** - Validate directory structure logic
- **Object store safety** - Preview what will be created in cloud storage
- **Regression testing** - Verify configs still parse correctly

## Comparison to sai3-bench

Both tools now support `--dry-run` with similar output:
- ✅ Config validation
- ✅ Backend detection
- ✅ Directory structure analysis
- ✅ Storage estimation
- ✅ Operation mix display (sai3-bench) / Training workload (dl-driver)

## Next Steps

1. ✅ Add `--dry-run` flag - DONE
2. ✅ Create small test configs - DONE
3. ⏭️ **Run small tests to verify functionality** (next task)
4. ⏭️ Verify object store behavior (S3/Azure/GCS)
5. ⏭️ Update documentation (DL-DRIVER_SETUP_GUIDE.md)
6. ⏭️ Add mode comparison table to docs

## Files Created/Modified

### Modified
- `dl-driver/crates/cli/src/main.rs`:
  - Added `dry_run` parameter to `Run` command
  - Added `display_config_summary()` function (250+ lines)
  - Call `display_config_summary()` and exit if `--dry-run`

### Created (Test Configs)
- `tests/dlio_configs/test_mode1_small_flat.yaml`
- `tests/dlio_configs/test_mode2_small_sharding.yaml`
- `tests/dlio_configs/test_mode3_small_hierarchical.yaml`

### Created (Full Configs)
- `tests/dlio_configs/resnet50_1host_mode1_flat.yaml`
- `tests/dlio_configs/resnet50_1host_mode2_sharding.yaml`
- `tests/dlio_configs/resnet50_1host_mode3_hierarchical.yaml`

### Also Available In (Container Examples)
- `Containers/sai3-multi/configs/AI-ML/dl-driver/*.yaml` (same configs, for multi-container testing)

## Build Status

✅ **Compiled successfully** (6.67s)
✅ **All 3 test configs validate with --dry-run**
✅ **Ready for execution testing**
