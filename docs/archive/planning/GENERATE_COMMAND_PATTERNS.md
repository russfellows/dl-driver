# Generate Command Usage Patterns

**Date:** November 2, 2025  
**Version:** 0.8.2 → 0.8.3

This document clarifies the different ways to generate data and control workflow phases in dl-driver.

---

## Overview

dl-driver supports **two approaches** for data generation:

1. **Standalone `generate` command** - Generate data only, no training
2. **Workflow control in config** - Control phases (generate + train) via `workflow:` section

Both approaches use the **same underlying code** (`run_data_generation()`), but provide different user experiences for different use cases.

---

## Approach 1: Standalone `generate` Command

**Use case:** Generate test data once, then run multiple training experiments on the same dataset.

### Command Syntax
```bash
dl-driver generate --config <config.yaml> [OPTIONS]
```

### Options
- `--verbose` - Show progress during generation
- `--skip-existing` - Skip generation if data folder already exists (TODO: not yet implemented)

### Example Config
```yaml
# test_data_generation_config.yaml
dataset:
  data_folder: "file:///tmp/my_dataset"
  format: "npz"
  num_files_train: 100
  record_length_bytes: 1048576  # 1 MB per file
  num_samples_per_file: 10

reader:
  data_loader: "pytorch"
  batch_size: 16
  read_threads: 4

# Workflow section is IGNORED by 'generate' command
workflow:
  generate_data: true  # Not checked
  train: false         # Not checked
```

### Usage Pattern
```bash
# Step 1: Generate data once
dl-driver generate --config test_data_generation_config.yaml --verbose

# Step 2: Run training multiple times (with different parameters)
dl-driver run --config train_only_config.yaml
dl-driver run --config train_only_config.yaml --pool-size 32
dl-driver run --config train_only_config.yaml --profile torch-like
```

### What Gets Generated
- Files created at `dataset.data_folder` location
- Number of files: `dataset.num_files_train`
- File size: `dataset.record_length_bytes × dataset.num_samples_per_file`
- Format: `dataset.format` (npz, hdf5, tfrecord)
- Directory structure: Based on `dataset.directory_tree` setting (Mode 1/2/3)

### Output Example
```
📦 Generating 100 files (0.10 GB total)...
✅ Generated 100 files (0.10 GB) in 0.02s @ 4043.2 MB/s
```

---

## Approach 2: Workflow-Controlled Phases (`run` command)

**Use case:** Single command to generate data AND run training, or control which phases execute.

### Workflow Configuration
The `workflow:` section in the config controls which phases execute:

```yaml
workflow:
  generate_data: true   # Phase 1: Generate synthetic data
  train: true           # Phase 2: Run training workload
  checkpoint: false     # Phase 3: Checkpointing (future)
  evaluation: false     # Phase 4: Evaluation (future)
```

### Pattern 1: Generate + Train (Full Pipeline)
```yaml
# minimal_config.yaml
model:
  name: my_workload

framework: pytorch

workflow:
  generate_data: true  # ✅ Generate data first
  train: true          # ✅ Then train on it

dataset:
  data_folder: file:///tmp/dlio_minimal_data
  format: npz
  num_files_train: 100
  record_length_bytes: 1048576

reader:
  data_loader: pytorch
  batch_size: 16
  read_threads: 4
  compute_threads: 2
  prefetch: 8
  shuffle: true

train:
  epochs: 3
  computation_time: 0.05
```

**Usage:**
```bash
dl-driver run --config minimal_config.yaml
```

**Output:**
```
📁 Phase 1: Data Generation
📦 Generating 100 files (0.10 GB total)...
✅ Generated 100 files (0.10 GB) in 0.02s @ 4043.2 MB/s

🚀 Phase 2: Training
📊 Phase: Training (MEASURED for AU calculation)
🏃 Epoch 1/3 starting...
✅ Epoch 1/3 complete: 7 batches, 100 samples, 104.9MB in 0.15s
...
```

### Pattern 2: Generate Only (via workflow)
```yaml
# generate_only_config.yaml
workflow:
  generate_data: true   # ✅ Generate data
  train: false          # ❌ Skip training

dataset:
  data_folder: file:///tmp/my_data
  format: npz
  num_files_train: 50
  record_length_bytes: 2097152  # 2 MB

# reader and train sections can be omitted or minimal
```

**Usage:**
```bash
dl-driver run --config generate_only_config.yaml
```

**Output:**
```
📁 Phase 1: Data Generation
📦 Generating 50 files (0.10 GB total)...
✅ Generated 50 files (0.10 GB) in 0.02s @ 410.2 MB/s
✅ DLIO workload completed successfully
```

### Pattern 3: Train Only (assumes data exists)
```yaml
# train_only_config.yaml
workflow:
  generate_data: false  # ❌ Skip generation
  train: true           # ✅ Train on existing data

dataset:
  data_folder: file:///tmp/existing_data  # MUST already exist!
  format: npz
  num_files_train: 50
  record_length_bytes: 2097152

reader:
  data_loader: pytorch
  batch_size: 16
  read_threads: 4

train:
  epochs: 5
  computation_time: 0.05
```

**Usage:**
```bash
# Data must already exist at /tmp/existing_data
dl-driver run --config train_only_config.yaml
```

**Output:**
```
🚀 Phase 2: Training
📊 Phase: Training (MEASURED for AU calculation)
🏃 Epoch 1/5 starting...
...
```

---

## Decision Matrix: Which Approach to Use?

| Scenario | Recommended Approach | Command |
|----------|---------------------|---------|
| Generate test data once, run many training experiments | **Standalone `generate`** | `generate --config data.yaml` |
| Quick end-to-end test (generate + train) | **Workflow (both true)** | `run --config combined.yaml` |
| Benchmark training on existing dataset | **Workflow (train only)** | `run --config train_only.yaml` |
| CI/CD pipeline with separate stages | **Standalone `generate`** | Stage 1: `generate`, Stage 2: `run` |
| Interactive experimentation | **Workflow (both true)** | Single command for convenience |
| Multi-host distributed testing | **Workflow (train only)** | Pre-generate data, then distribute |

---

## Key Differences

### Standalone `generate` Command
✅ **Pros:**
- Explicitly clear intent (only generating data)
- Separate command for separate concern
- Can use `--skip-existing` flag (when implemented)
- Better for scripting/automation with separate stages

❌ **Cons:**
- Requires two commands for full workflow
- Need to manage two config files (or ensure workflow flags don't interfere)

### Workflow-Controlled `run` Command
✅ **Pros:**
- Single command for full pipeline
- Config explicitly controls all phases
- Easy to toggle phases for testing
- Natural for DLIO compatibility (matches DLIO's workflow concept)

❌ **Cons:**
- Less obvious that data generation happens
- Can't pass generation-specific flags (like `--skip-existing`)
- More complex logic in single command

---

## Config File Organization

### For Standalone `generate` Approach
```
project/
├── configs/
│   ├── data_generation.yaml      # Only dataset and reader sections
│   ├── train_baseline.yaml       # workflow.generate_data = false
│   ├── train_optimized.yaml      # workflow.generate_data = false
│   └── train_distributed.yaml    # workflow.generate_data = false
└── scripts/
    ├── 01_generate_data.sh       # dl-driver generate --config data_generation.yaml
    ├── 02_train_baseline.sh      # dl-driver run --config train_baseline.yaml
    └── 03_train_optimized.sh     # dl-driver run --config train_optimized.yaml
```

### For Workflow-Controlled Approach
```
project/
├── configs/
│   ├── full_pipeline.yaml        # workflow: generate=true, train=true
│   ├── generate_only.yaml        # workflow: generate=true, train=false
│   └── train_only.yaml           # workflow: generate=false, train=true
└── scripts/
    ├── run_full_test.sh          # dl-driver run --config full_pipeline.yaml
    ├── generate_data.sh          # dl-driver run --config generate_only.yaml
    └── train_only.sh             # dl-driver run --config train_only.yaml
```

---

## Command-Line Override Behavior

### Important Note
Neither command has flags to override the `workflow:` section settings. The workflow control is **config-only**.

### No Command-Line Overrides
```bash
# These don't exist (would be confusing):
dl-driver run --config test.yaml --skip-generation    # ❌ NOT AVAILABLE
dl-driver run --config test.yaml --skip-training      # ❌ NOT AVAILABLE
dl-driver run --config test.yaml --generate-only     # ❌ NOT AVAILABLE
```

### To Change Workflow Behavior
**Option 1:** Edit the config file
```bash
# Edit workflow section in config
vim config.yaml  # Change generate_data: true/false

dl-driver run --config config.yaml
```

**Option 2:** Create separate config variants
```bash
# Use different configs for different workflows
dl-driver run --config config_generate.yaml    # generate_data=true, train=false
dl-driver run --config config_train.yaml       # generate_data=false, train=true
dl-driver run --config config_full.yaml        # generate_data=true, train=true
```

**Option 3:** Use `generate` command explicitly
```bash
# Ignore workflow settings, just generate
dl-driver generate --config config.yaml
```

---

## Testing and Validation

### Dry-Run Mode
Both approaches support validation without execution:

```bash
# Validate generate command (shows what would be generated)
dl-driver validate --config data_generation.yaml

# Validate full workflow (shows all phases)
dl-driver validate --config full_pipeline.yaml

# Validate with dry-run flag (same output as validate)
dl-driver run --config full_pipeline.yaml --dry-run
```

### Example Dry-Run Output
```
╔═══════════════════════════════════════════════════════════════════════╗
║         DL-DRIVER CONFIGURATION VALIDATION & TEST SUMMARY             ║
╚═══════════════════════════════════════════════════════════════════════╝

✅ Config file parsed successfully: tests/dlio_configs/minimal_config.yaml

┌─ Model Configuration ────────────────────────────────────────────────┐
│ Model Name:   my_workload
└──────────────────────────────────────────────────────────────────────┘

Framework: pytorch

┌─ Workflow Phases ────────────────────────────────────────────────────┐
│ Generate Data:  ✅ YES     <-- Shows what will execute
│ Training:       ✅ YES
│ Checkpoint:     ❌ NO
│ Evaluation:     ❌ NO
└──────────────────────────────────────────────────────────────────────┘
...
```

---

## Recommendations

### For v0.8.3 Documentation Updates

1. **Keep both approaches** - They serve different use cases
2. **Document decision matrix** - Help users choose the right approach
3. **Clarify workflow control** - Make it clear that `workflow:` section controls `run` command phases
4. **Show examples** - Provide config examples for each pattern
5. **Note limitations** - Document that `generate` command ignores `workflow:` section

### User Guide Additions

Add a "Data Generation Patterns" section to USER_GUIDE.md showing:
- When to use `generate` vs `run` with workflow control
- How to set up multi-stage pipelines
- How to validate before execution
- Common config patterns for each approach

### CLI Help Text Improvements

Update help text to clarify:
```
Commands:
  run          Run DLIO workload (respects workflow.generate_data and workflow.train settings)
  validate     Validate config and show execution summary (alias for --dry-run)
  generate     Generate synthetic dataset (ignores workflow settings, always generates)
  distributed  Run distributed workload across multiple agents
```

---

## Future Enhancements

### Potential Command-Line Overrides (Future)
If we want to add phase control via CLI flags:

```bash
# Proposed syntax (not implemented):
dl-driver run --config test.yaml --phases generate,train      # Override workflow
dl-driver run --config test.yaml --phases train               # Train only
dl-driver run --config test.yaml --skip-phase generate        # Skip generation
```

**Decision for v0.8.3:** Don't add these yet. Keep it simple with config-based control.

### Skip-Existing Implementation
Complete the `--skip-existing` flag for `generate` command:

```rust
// TODO in run_generate_only()
if skip_existing {
    let uri = dlio_config.dataset.data_folder;
    let store = store_for_uri(&uri, Default::default()).await?;
    
    // Check if any files exist
    let list_result = store.list(None).await;
    if !list_result.is_empty() {
        info!("Data folder already exists and is not empty. Skipping generation.");
        return Ok(());
    }
}
```

---

## Conclusion

Both approaches are **intentionally kept** because they serve different needs:

- **`generate` command:** Explicit, scriptable, separate-stage workflows
- **`run` with workflow control:** Convenience, single-command testing, DLIO compatibility

The key is clear documentation so users understand when to use each approach.
