# dl-driver Update Plan - Add Multi-Endpoint Support

**Date**: November 7, 2025

## Current Project Versions (as of November 7, 2025)

| Project | Current Version | Status |
|---------|----------------|--------|
| **s3dlio** | v0.9.16 | ✅ Latest (released today) |
| **sai3-bench** | v0.7.4 | ✅ Latest (released today) |
| **dl-driver** | v0.8.4 | ⚠️ Using s3dlio v0.9.12 (3 releases behind) |

## s3dlio Feature Availability

| Feature | Version Added | Status |
|---------|---------------|--------|
| Multi-endpoint support (`MultiEndpointStore`) | v0.9.14 | ✅ Available |
| S3 URI endpoint parsing | v0.9.15 | ✅ Available |
| Optional op-log sorting | v0.9.16 | ✅ Available (not needed for dl-driver) |
| Load balancing strategies (RoundRobin, LeastConnections) | v0.9.14 | ✅ Available |
| Per-endpoint statistics | v0.9.14 | ✅ Available |

## Key Insight

**Multi-endpoint support is ALREADY IMPLEMENTED in s3dlio v0.9.14!** We just need to:
1. Update dl-driver's s3dlio dependency
2. Add configuration support for multi-endpoint URIs
3. Update documentation

**Op-log support is NOT needed** - dl-driver's purpose is different from sai3-bench. If we need I/O trace replay, we use sai3-bench.

## Current State Analysis

**dl-driver on s3dlio v0.9.12** (November 2024):
- ❌ Missing: Multi-endpoint support (v0.9.14)
- ❌ Missing: S3 URI endpoint parsing (v0.9.15)
- ❌ Missing: Latest bug fixes and optimizations

**Code Quality**:
- ✅ Already using `s3dlio::object_store::store_for_uri` (good!)
- ✅ Using `ObjectStore` trait from s3dlio (good!)
- ❓ Need to check: Any custom ObjectStore wrappers or duplicated logic?

## Phase 1: Update Dependencies (Comprehensive)

## Phase 1: Update Dependencies (Comprehensive)

### 1.1 Update s3dlio to v0.9.16

Update all crate `Cargo.toml` files to use s3dlio v0.9.16:

**Files to update:**
- `crates/core/Cargo.toml`
- `crates/formats/Cargo.toml`
- `crates/frameworks/Cargo.toml`
- `crates/cli/Cargo.toml`

**Change:**
```toml
# OLD (v0.9.12):
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", tag = "v0.9.12" }

# NEW (v0.9.16):
s3dlio = { git = "https://github.com/russfellows/s3dlio.git", tag = "v0.9.16" }
```

**What this brings:**
- ✅ Multi-endpoint support (`MultiEndpointStore`, `LoadBalanceStrategy`)
- ✅ S3 URI endpoint parsing (`parse_s3_uri_full`)
- ✅ Per-endpoint statistics tracking
- ✅ Bug fixes and performance improvements from 4 releases

### 1.2 Comprehensive Dependency Update

Run full dependency update to get latest compatible versions:

```bash
cd dl-driver
cargo update  # Updates Cargo.lock with latest compatible dependencies
cargo build --release  # Verify build succeeds
cargo test  # Verify all 123 tests still pass
```

**This updates ALL dependencies**, not just s3dlio:
- tokio, serde, anyhow, etc.
- Ensures latest bug fixes and security patches
- Maintains compatibility (respects Cargo.toml version constraints)

**Expected Build Time**: 2-5 minutes (full rebuild with new dependencies)

### 1.3 Verify Clean Build

```bash
# Check for warnings
cargo clippy --all-targets --all-features

# Verify test suite
cargo test --workspace

# Expected: All 123 tests passing, zero warnings
```

**Time Estimate**: 20-30 minutes total

## Phase 2: Add Multi-Endpoint Configuration Support

### 2.1 Update Configuration Schema

Update `crates/core/src/config.rs` to support multi-endpoint URIs:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetConfig {
    /// Data folder URI or multi-endpoint specification (v0.8.5+)
    /// 
    /// **Single endpoint** (backward compatible):
    ///   data_folder: "s3://bucket/data/"
    /// 
    /// **Multiple endpoints** (comma-separated):
    ///   data_folder: "s3://10.0.0.1:9000/bucket/,s3://10.0.0.2:9000/bucket/,s3://10.0.0.3:9000/bucket/"
    /// 
    /// **Template expansion** (not yet implemented in s3dlio):
    ///   data_folder: "s3://10.0.0.{1...8}:9000/bucket/data/"
    /// 
    /// **Multiple file systems**:
    ///   data_folder: "file:///mnt/storage1/data/,file:///mnt/storage2/data/"
    #[serde(alias = "data_folder")]
    pub data_folder_uri: String,
    
    /// Load balancing strategy for multi-endpoint (v0.8.5+)
    /// Options: "round-robin", "least-connections" (default)
    #[serde(default = "default_endpoint_strategy")]
    pub endpoint_strategy: String,
    
    // ... existing fields ...
}

fn default_endpoint_strategy() -> String {
    "least-connections".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointConfig {
    /// Checkpoint folder URI or multi-endpoint specification (v0.8.5+)
    pub checkpoint_folder: String,
    
    /// Load balancing strategy for checkpoint storage (v0.8.5+)
    #[serde(default = "default_endpoint_strategy")]
    pub endpoint_strategy: String,
    
    // ... existing fields ...
}
```

### 2.2 Update Workload Store Creation

Update `crates/core/src/workload.rs` to use multi-endpoint stores:

```rust
use s3dlio::multi_endpoint::{MultiEndpointStore, LoadBalanceStrategy};
use s3dlio::object_store::store_for_uri;

/// Create ObjectStore for dataset, handling multi-endpoint if configured
pub fn create_dataset_store(config: &DatasetConfig) -> Result<Box<dyn ObjectStore>> {
    let uri = &config.data_folder_uri;
    
    // Check if multi-endpoint (contains comma)
    if uri.contains(',') {
        let uris: Vec<String> = uri.split(',')
            .map(|s| s.trim().to_string())
            .collect();
        
        let strategy = match config.endpoint_strategy.as_str() {
            "round-robin" => LoadBalanceStrategy::RoundRobin,
            "least-connections" => LoadBalanceStrategy::LeastConnections,
            _ => {
                warn!("Unknown endpoint strategy '{}', using least-connections", 
                      config.endpoint_strategy);
                LoadBalanceStrategy::LeastConnections
            }
        };
        
        info!("Creating multi-endpoint store with {} endpoints (strategy: {})", 
              uris.len(), config.endpoint_strategy);
        
        let store = MultiEndpointStore::new(uris, strategy, None)?;
        Ok(Box::new(store))
    } else {
        // Single endpoint - use standard factory
        store_for_uri(uri).context("Failed to create single-endpoint store")
    }
}
```

### 2.3 Update Checkpoint Plugin

Update `crates/core/src/plugins/checkpoint.rs` similarly:

```rust
/// Create checkpoint store with multi-endpoint support
pub fn create_checkpoint_store(config: &CheckpointConfig) -> Result<Box<dyn ObjectStore>> {
    let uri = &config.checkpoint_folder;
    
    if uri.contains(',') {
        let uris: Vec<String> = uri.split(',').map(|s| s.trim().to_string()).collect();
        
        let strategy = match config.endpoint_strategy.as_str() {
            "round-robin" => LoadBalanceStrategy::RoundRobin,
            "least-connections" => LoadBalanceStrategy::LeastConnections,
            _ => LoadBalanceStrategy::LeastConnections,
        };
        
        let store = MultiEndpointStore::new(uris, strategy, None)?;
        Ok(Box::new(store))
    } else {
        store_for_uri(uri).context("Failed to create checkpoint store")
    }
}
```

**Time Estimate**: 1 hour

## Phase 3: Add Per-Endpoint Metrics Reporting

### 3.1 Export Endpoint Statistics

Update metrics reporting to include per-endpoint stats when using multi-endpoint stores:

```rust
// In workload completion reporting
if let Some(multi_store) = store.downcast_ref::<MultiEndpointStore>() {
    let endpoint_stats = multi_store.get_stats();
    
    info!("=== Per-Endpoint Statistics ===");
    for stat in endpoint_stats {
        info!("  {}: {} requests, {} MB, {} errors", 
              stat.uri,
              stat.requests_total,
              stat.bytes_total / 1_048_576,
              stat.errors_total);
    }
    
    // Export to TSV if results directory exists
    if let Some(results_dir) = &config.results_directory {
        export_endpoint_stats(&endpoint_stats, results_dir)?;
    }
}
```

### 3.2 Add TSV Export Function

```rust
/// Export per-endpoint statistics to TSV
pub fn export_endpoint_stats(
    stats: &[EndpointStatsSummary],
    results_dir: &Path,
) -> Result<()> {
    let filepath = results_dir.join("endpoint_results.tsv");
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_path(&filepath)?;
    
    wtr.write_record(&["endpoint", "requests", "bytes_mb", "errors"])?;
    
    for stat in stats {
        wtr.write_record(&[
            &stat.uri,
            &stat.requests_total.to_string(),
            &format!("{:.2}", stat.bytes_total as f64 / 1_048_576.0),
            &stat.errors_total.to_string(),
        ])?;
    }
    
    wtr.flush()?;
    info!("Endpoint statistics exported to {}", filepath.display());
    Ok(())
}
```

**Time Estimate**: 30 minutes

## Phase 4: Testing & Documentation

### 4.1 Add Example Configurations

**Simple multi-endpoint example** (`tests/dlio_configs/multi_endpoint_simple.yaml`):
```yaml
dataset:
  # Multi-endpoint S3 storage (3 endpoints, round-robin)
  data_folder: "s3://10.0.0.1:9000/ml-data/,s3://10.0.0.2:9000/ml-data/,s3://10.0.0.3:9000/ml-data/"
  endpoint_strategy: "round-robin"
  record_length: 1048576
  num_files_train: 1000

model:
  framework: "pytorch"
  layers: [2048, 1024, 512]

workload:
  epochs: 3
  batch_size: 64
  workers: 8
```

**Advanced multi-endpoint example** (`tests/dlio_configs/multi_endpoint_advanced.yaml`):
```yaml
dataset:
  # Multi-endpoint with least-connections balancing
  data_folder: "s3://fast-nvme:9000/data/,s3://medium-ssd:9000/data/,s3://slow-hdd:9000/data/"
  endpoint_strategy: "least-connections"
  record_length: 4194304  # 4MB records
  num_files_train: 10000

checkpoint:
  # Separate checkpoint endpoints for resilience
  checkpoint_folder: "s3://backup1:9000/checkpoints/,s3://backup2:9000/checkpoints/"
  endpoint_strategy: "round-robin"
  checkpoint_after_epoch: 1

model:
  framework: "pytorch"
  layers: [4096, 2048, 1024, 512]

workload:
  epochs: 10
  batch_size: 128
  workers: 16
```

### 4.2 Update Documentation

**README.md** - Add v0.8.5 release notes:
```markdown
## Latest Release: v0.8.5 (November 2025)

### Multi-Endpoint Storage Support

- **Load-balanced I/O**: Distribute requests across multiple storage endpoints
- **Flexible configuration**: Comma-separated URIs for S3, file systems, or mixed
- **Strategy options**: Round-robin or least-connections balancing
- **Per-endpoint metrics**: Track performance of each endpoint independently

Example:
\`\`\`yaml
dataset:
  data_folder: "s3://10.0.0.1:9000/data/,s3://10.0.0.2:9000/data/"
  endpoint_strategy: "least-connections"
\`\`\`
```

**docs/Changelog.md** - Comprehensive entry:
```markdown
## [0.8.5] - 2025-11-07

### Multi-Endpoint Storage Support

Add support for load-balanced I/O across multiple storage endpoints,
enabling better utilization of multi-NIC storage systems, NUMA-aware
configurations, and high-availability setups.

**Features:**
- Multi-endpoint URIs via comma-separated lists
- Load balancing strategies (round-robin, least-connections)
- Per-endpoint performance metrics
- Checkpoint storage across multiple endpoints
- Backward compatible (single endpoints work unchanged)

**Dependencies:**
- Updated s3dlio to v0.9.16 (from v0.9.12)
- Comprehensive dependency update via `cargo update`
```

**docs/USER_GUIDE.md** - Usage examples:
```markdown
### Multi-Endpoint Storage

dl-driver supports distributing I/O across multiple storage endpoints
for improved performance and availability.

#### Configuration

Use comma-separated URIs in `data_folder` or `checkpoint_folder`:

\`\`\`yaml
dataset:
  data_folder: "s3://host1:9000/data/,s3://host2:9000/data/"
  endpoint_strategy: "least-connections"  # or "round-robin"
\`\`\`

#### Load Balancing Strategies

- **least-connections** (default): Routes to endpoint with fewest active requests
  - Best for heterogeneous storage (fast/slow disks)
  - Adapts to varying endpoint performance
  
- **round-robin**: Cycles through endpoints sequentially
  - Best for uniform storage with similar performance
  - Lowest overhead, predictable distribution

#### Use Cases

1. **Multi-NIC storage**: Spread load across network interfaces
2. **NUMA optimization**: Match storage endpoints to CPU sockets
3. **High availability**: Continue if one endpoint fails
4. **Tiered storage**: Mix fast NVMe and slower HDD endpoints
```

### 4.3 Test Multi-Endpoint Functionality

```bash
cd dl-driver

# Test with file:// backend (easy to test locally)
# Create multiple mount points or directories
mkdir -p /tmp/endpoint1 /tmp/endpoint2 /tmp/endpoint3

# Create test config
cat > /tmp/multi_endpoint_test.yaml <<EOF
dataset:
  data_folder: "file:///tmp/endpoint1/,file:///tmp/endpoint2/,file:///tmp/endpoint3/"
  endpoint_strategy: "round-robin"
  record_length: 1048576
  num_files_train: 100
  
model:
  framework: "pytorch"
  layers: [512, 256]
  
workload:
  epochs: 1
  batch_size: 16
  workers: 4
EOF

# Run test
cargo run --release -- run --config /tmp/multi_endpoint_test.yaml

# Verify files distributed across endpoints
ls -la /tmp/endpoint1/ /tmp/endpoint2/ /tmp/endpoint3/
```

### 4.4 Verify Test Suite

```bash
# Run full test suite
cargo test --workspace

# Expected: All 123 tests passing
# No new failures from dependency updates
```

**Time Estimate**: 2 hours

## Implementation Timeline

### Immediate (Can do now - ~3 hours total):

1. ✅ **Phase 1**: Update s3dlio to v0.9.16 + comprehensive `cargo update` (30 min)
   - Updates all 4 crate Cargo.toml files
   - Runs full dependency update
   - Verifies build and tests

2. ✅ **Phase 2**: Add multi-endpoint configuration support (1 hour)
   - Update DatasetConfig and CheckpointConfig
   - Implement multi-endpoint store creation logic
   - Add comma-separated URI parsing

3. ✅ **Phase 3**: Add per-endpoint metrics reporting (30 min)
   - Export endpoint statistics to TSV
   - Add console output for endpoint performance

4. ✅ **Phase 4**: Testing & documentation (1 hour)
   - Create example configurations
   - Update README, Changelog, USER_GUIDE
   - Test multi-endpoint functionality
   - Verify all 123 tests still pass

### Version Planning

**Target**: dl-driver v0.8.5 (November 7, 2025)

**Changes from v0.8.4**:
- ✅ Update s3dlio dependency: v0.9.12 → v0.9.16
- ✅ Add multi-endpoint storage support
- ✅ Add per-endpoint metrics
- ✅ Comprehensive dependency updates
- ✅ Documentation and examples

**Breaking Changes**: None (backward compatible)
- Single-endpoint URIs work unchanged
- Multi-endpoint is opt-in feature (detected by comma separator)

## Expected Benefits

### Performance
- **Multi-endpoint I/O parallelism**: Utilize multiple storage paths simultaneously
- **NUMA optimization**: Match endpoints to CPU sockets for cache locality
- **Network utilization**: Spread load across multiple NICs or storage controllers
- **No overhead for single-endpoint**: Backward compatible with zero cost

### Operational
- **High availability**: Continue operation if one endpoint fails (with degraded performance)
- **Tiered storage**: Mix fast and slow endpoints based on workload needs
- **Consistent configuration**: Same syntax as sai3-bench for cross-tool compatibility
- **Per-endpoint visibility**: Track which endpoints are performing well or having issues

### Code Quality
- **Latest s3dlio features**: Multi-endpoint support, URI parsing, bug fixes
- **Up-to-date dependencies**: Latest security patches and improvements
- **Leverage existing code**: No need to implement load balancing ourselves
- **Ecosystem consistency**: Same approach as sai3-bench v0.7.4

## Risk Assessment

**Low Risk**:
- ✅ s3dlio v0.9.16 update (backward compatible, well-tested)
- ✅ Multi-endpoint support (already proven in s3dlio v0.9.14)
- ✅ Cargo update (respects version constraints in Cargo.toml)
- ✅ Backward compatibility (single URIs work unchanged)

**Medium Risk**:
- ⚠️ Dependency updates might introduce subtle behavior changes
  - **Mitigation**: Run full test suite (123 tests) after update
- ⚠️ Multi-endpoint parsing edge cases
  - **Mitigation**: Start with simple comma-split, add validation

**No High Risks Identified**

## Success Criteria

- ✅ All 123 dl-driver tests passing after updates
- ✅ Multi-endpoint configuration parses correctly
- ✅ Load balancing strategies work (round-robin, least-connections)
- ✅ Per-endpoint statistics exported to TSV
- ✅ Backward compatibility maintained (existing configs unchanged)
- ✅ Zero compilation warnings
- ✅ Documentation comprehensive with examples
- ✅ Example configs provided and tested

## Code Quality Review (Optional)

If time permits, search for potential duplications:

```bash
cd dl-driver

# Check for custom ObjectStore implementations
rg "impl.*ObjectStore" --type rust

# Check for URI parsing duplicates
rg "parse.*uri|scheme.*from" --type rust | grep -v "s3dlio::"

# Check for retry/error handling that s3dlio might provide
rg "retry|backoff" --type rust | grep -v "s3dlio::"
```

**If duplications found**: Consolidate to use s3dlio types (like we did with sai3-bench OpLogEntry)

## Next Steps - Execution Order

1. **Start**: Update s3dlio dependency in all 4 crates
2. **Update**: Run `cargo update` for comprehensive dependency refresh
3. **Build**: Verify clean build with `cargo build --release`
4. **Test**: Ensure all 123 tests pass with `cargo test --workspace`
5. **Implement**: Add multi-endpoint configuration support
6. **Metrics**: Add per-endpoint statistics reporting
7. **Document**: Update README, Changelog, USER_GUIDE
8. **Examples**: Create and test example configurations
9. **Verify**: Final test run and code quality check
10. **Version**: Bump to v0.8.5 and update documentation
11. **Commit**: Create comprehensive commit message
12. **Push**: Push to feature branch and create PR

---

**Questions Answered:**

1. ✅ **Should we proceed with Phase 1 (s3dlio update) immediately?**
   - YES - Update from v0.9.12 to v0.9.16 to get multi-endpoint support
   
2. ✅ **Do you want op-log support integrated like sai3-bench?**
   - NO - dl-driver's purpose is different; use sai3-bench for I/O replay
   
3. ✅ **Are there any dl-driver-specific ObjectStore wrappers we should review?**
   - Optional code quality review to check for duplications
   
4. ✅ **Should we wait to implement multi-endpoint until s3dlio Phase 1 is complete?**
   - NO - Multi-endpoint is ALREADY implemented in s3dlio v0.9.14!

**Ready to proceed?** Start with Phase 1 - update dependencies!

