# CLI Command Naming - Proposal for v0.8.3+

**Date:** November 2, 2025  
**Issue:** Current command structure is confusing regarding phase control

---

## Current State (Confusing)

### Commands
- `generate` - Generates data only (ignores workflow config)
- `run` - Can do BOTH generate and/or train (controlled by workflow config)
- `validate` / `--dry-run` - Validates config (aliases)
- `distributed` - Multi-host coordination

### User Confusion
❌ **"run" sounds like it only trains, not generates**  
❌ **Two ways to generate data (command vs workflow) is unclear**  
❌ **No explicit "train-only" command**  
❌ **Phase control is config-only, no CLI override**

---

## Workflow Phases (DLIO Compatibility)

Per DLIO spec and current implementation, there are **4 phases**:

1. **`generate_data`** - Generate synthetic dataset
2. **`train`** - Training/data loading workload (the main benchmark)
3. **`checkpoint`** - Checkpointing I/O (optional, planned)
4. **`evaluation`** - Evaluation phase (optional, planned)

Currently only phases 1 and 2 are implemented. Phases 3 and 4 are:
- ✅ Defined in config schema
- ✅ Displayed in dry-run validation
- ❌ NOT executed (commented out in code)

---

## Proposal: Option A - Explicit Phase Commands

### New Command Structure
```bash
# Phase-specific commands (explicit, clear intent)
dl-driver generate --config data.yaml     # Phase 1 only
dl-driver train --config train.yaml       # Phase 2 only (requires existing data)
dl-driver checkpoint --config ckpt.yaml   # Phase 3 only (future)
dl-driver evaluate --config eval.yaml     # Phase 4 only (future)

# Full workflow (respects workflow: section)
dl-driver run --config full.yaml          # Executes enabled phases from config

# Utilities
dl-driver validate --config test.yaml     # Validate config
dl-driver distributed run --config d.yaml # Multi-host execution
```

### Behavior

**`generate` command:**
- ✅ Already exists
- Generates data only, ignores `workflow:` section
- Options: `--verbose`, `--skip-existing`

**`train` command (NEW):**
- Trains on **existing data** only
- Equivalent to `run` with `workflow: {generate_data: false, train: true}`
- Fails if data folder doesn't exist
- Options: Same as current `run` (--pool-size, --accelerators, --profile, etc.)

**`run` command (NO CHANGE):**
- Respects all `workflow:` flags
- Can execute any combination of phases
- Default: All enabled phases

### Migration Path
```bash
# Old way (still works)
dl-driver generate --config data.yaml
dl-driver run --config train_only.yaml  # workflow.generate_data = false

# New way (clearer)
dl-driver generate --config data.yaml
dl-driver train --config train_only.yaml  # No need to set workflow flags
```

### Pros
✅ **Clear intent** - Command name matches the action  
✅ **Backward compatible** - All existing commands still work  
✅ **Reduces config complexity** - Don't need workflow flags for single-phase operations  
✅ **Future-proof** - Easy to add `checkpoint` and `evaluate` commands  
✅ **Better error messages** - "train: data folder not found" vs "run: failed"

### Cons
❌ **More commands** - Users need to learn 2+ new commands  
❌ **Redundancy** - `train` is just `run` with specific workflow flags

---

## Proposal: Option B - CLI Phase Overrides

### New Flag Structure
```bash
# Override workflow config with CLI flag
dl-driver run --config test.yaml --phases generate
dl-driver run --config test.yaml --phases train
dl-driver run --config test.yaml --phases generate,train
dl-driver run --config test.yaml --phases all

# Skip specific phases
dl-driver run --config test.yaml --skip-phase generate
dl-driver run --config test.yaml --skip-phase checkpoint
```

### Behavior
- `--phases` **overrides** `workflow:` section entirely
- `--skip-phase` **removes** specific phase from workflow
- If neither flag specified, uses `workflow:` section

### Examples
```bash
# Config has workflow: {generate_data: true, train: true}

# Override to train only
dl-driver run --config test.yaml --phases train

# Override to generate only
dl-driver run --config test.yaml --phases generate

# Use config's workflow settings (both phases)
dl-driver run --config test.yaml
```

### Pros
✅ **Flexible** - Can override config without editing  
✅ **Single command** - Don't need separate `train`/`generate` commands  
✅ **Scripting-friendly** - Easy to test different phase combinations

### Cons
❌ **Complex precedence** - CLI vs config can be confusing  
❌ **Flag explosion** - Multiple ways to do the same thing  
❌ **Still keeps confusing `generate` command** - Two ways to generate

---

## Proposal: Option C - Rename `generate` Command

### Rename Structure
```bash
# Rename 'generate' to 'prepare' (or 'setup')
dl-driver prepare --config data.yaml      # Prepare data (generate only)
dl-driver run --config test.yaml          # Run workload (all phases)

# Or deprecate 'generate' entirely
# (just use: dl-driver run --config gen_only.yaml)
```

### Rationale
- **"prepare"** clearly indicates setup/prerequisite step
- **"run"** clearly indicates the actual benchmark execution
- Reduces confusion about what `run` does

### Pros
✅ **Clearer terminology** - "prepare data, then run benchmark"  
✅ **Minimal change** - Just rename one command

### Cons
❌ **Breaking change** - Existing scripts using `generate` break  
❌ **Doesn't solve phase control** - Still need workflow flags

---

## Recommendation: **Option A** (Explicit Phase Commands)

### Rationale
1. **Matches user mental model** - "I want to train" → `dl-driver train`
2. **Reduces config complexity** - Don't need workflow flags for simple cases
3. **Future-proof** - Easy to add checkpoint/evaluate commands
4. **Backward compatible** - Keep all existing commands
5. **Better error messages** - Phase-specific failures

### Implementation Plan

#### Step 1: Add `train` command
```rust
Commands::Train {
    config,
    // ... same options as Run
} => run_train_only(&config, ...).await,
```

#### Step 2: Create `run_train_only()` wrapper
```rust
async fn run_train_only(config_path: &Path, ...) -> Result<()> {
    // Load config
    let mut dlio_config = DlioConfig::from_yaml(&yaml_content)?;
    
    // Force train-only workflow
    if let Some(ref mut workflow) = dlio_config.workflow {
        workflow.generate_data = Some(false);
        workflow.train = Some(true);
    }
    
    // Check data exists
    let uri = dlio_config.dataset.data_folder;
    check_data_exists(&uri)?;
    
    // Call existing run_unified_dlio
    run_unified_dlio(config_path, &dlio_config, ...).await
}
```

#### Step 3: Update help text
```
Commands:
  generate     Generate synthetic dataset (Phase 1 only)
  train        Run training workload on existing data (Phase 2 only)  [NEW]
  run          Run full DLIO workload (respects workflow configuration)
  validate     Validate config and show execution summary
  distributed  Run distributed workload across multiple agents
```

#### Step 4: Update documentation
- USER_GUIDE.md: Add `train` command examples
- GENERATE_COMMAND_PATTERNS.md: Update with `train` command
- README.md: Show `train` in quick start

#### Step 5: Add checkpoint/evaluate placeholders (future)
```rust
Commands::Checkpoint { config } => {
    return Err(anyhow::anyhow!(
        "Checkpoint command not yet implemented. Use 'run' with workflow.checkpoint: true"
    ));
}
```

---

## Alternative: Do Nothing (Keep Current)

### Argument for Status Quo
- Current system **works correctly**
- Documentation can clarify the confusion
- No breaking changes needed
- Users can adapt with better docs

### Counterargument
- **CLI should be intuitive** - shouldn't need extensive docs to understand
- **Command names matter** - `generate` vs `run` is inherently confusing
- **Better now than later** - Breaking changes acceptable before v1.0

---

## Decision Points

### For v0.8.3 (Current Release)
**Minimal change:** Just improve documentation
- Update help text to clarify `generate` vs `run`
- Add examples showing workflow control
- Document the 4 phases clearly

### For v0.9.0 (Next Release)
**Add `train` command** (Option A)
- Keep backward compatibility
- Add new explicit commands
- Update all documentation

### For v1.0.0 (Major Release)
**Optionally deprecate `generate`**
- If users prefer `train` command, deprecate `generate`
- Or keep both if both are useful

---

## User Feedback Needed

### Questions for Users
1. Is `dl-driver train` more intuitive than `dl-driver run` with workflow flags?
2. Should `generate` be renamed to `prepare` or `setup`?
3. Do you want CLI phase overrides (`--phases train,generate`)?
4. Would you use separate `checkpoint` and `evaluate` commands?

---

## Summary

**Current Issue:**
- `generate` command name doesn't clarify it's separate from `run`
- `run` command name doesn't indicate it can also generate
- No explicit "train-only" command

**Recommended Solution (v0.9.0):**
- Add `train` command for explicit train-only execution
- Keep `generate` and `run` for backward compatibility
- Update documentation to clarify all three commands
- Future: Add `checkpoint` and `evaluate` commands

**Quick Win (v0.8.3):**
- Improve help text and documentation NOW
- Defer command additions to v0.9.0
