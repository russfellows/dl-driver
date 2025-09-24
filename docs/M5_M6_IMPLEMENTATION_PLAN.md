# M5 & M6 Implementation Plan: Plugins and MLPerf Readiness

## Current Status Assessment ✅

### ✅ **What We Have:**
1. **Checkpoint Configuration Support**: Full DLIO checkpoint config parsing in `CheckpointConfig`
2. **Basic Metrics Collection**: `Metrics` and `WorkloadMetrics` with throughput calculation
3. **Framework Integration**: PyTorch, TensorFlow, JAX integration complete
4. **DLIO Configs**: 5 stock MLCommons DLIO configs (UNet3D, BERT, ResNet, CosmoFlow, minimal)
5. **Multi-Backend Support**: All 4 backends (File, DirectIO, S3, Azure) verified

### ❌ **What We Need:**

## 🔌 M5 - Plugins (Checkpointing)

### **Missing Components:**
1. **Plugin Trait System**: Generic plugin interface that can be toggled from YAML
2. **s3dlio CheckpointStore Integration**: Actual checkpoint implementation using s3dlio
3. **Python Checkpoint Hooks**: Hook points for PyTorch and TensorFlow training loops
4. **Compression Support**: zstd compression for checkpoint round-trips

### **Implementation Plan:**

#### **Phase 1: Plugin Infrastructure**
```rust
// crates/core/src/plugins/mod.rs
pub trait Plugin: Send + Sync {
    async fn initialize(&mut self, config: &DlioConfig) -> Result<()>;
    async fn before_epoch(&mut self, epoch: u32) -> Result<()>;
    async fn after_epoch(&mut self, epoch: u32) -> Result<()>;
    async fn before_step(&mut self, step: u32) -> Result<()>;
    async fn after_step(&mut self, step: u32) -> Result<()>;
    async fn finalize(&mut self) -> Result<()>;
}

// Checkpoint Plugin Implementation
pub struct CheckpointPlugin {
    store: s3dlio::CheckpointStore,
    config: CheckpointConfig,
    step_count: u32,
}
```

#### **Phase 2: Python Hook Integration**
```python
# crates/py_api/src/frameworks/pytorch.py
class DlioPyTorchDataset:
    def __init__(self, config_dict, plugins=None):
        self.plugins = plugins or []
        
    async def training_step_hook(self, step, model_state):
        for plugin in self.plugins:
            await plugin.after_step(step, model_state)
```

#### **Phase 3: s3dlio CheckpointStore Integration**
- Use s3dlio's existing `CheckpointStore` for streaming, multipart uploads
- Add compression support (zstd) for checkpoint files
- Support all 4 backends (File, DirectIO, S3, Azure)

---

## 🏆 M6 - MLPerf Readiness

### **Missing Components:**
1. **End-to-End DLIO Runner**: Run stock DLIO configs unmodified
2. **MLPerf Metrics Collection**: Per-epoch and per-stage throughput capture
3. **Seeded Access-Order Parity**: Ensure deterministic batch ordering
4. **MLPerf Report Generation**: JSON/CSV output aligned with MLPerf schema

### **Implementation Plan:**

#### **Phase 1: DLIO End-to-End Runner**
```rust
// crates/core/src/mlperf/runner.rs
pub struct DlioRunner {
    config: DlioConfig,
    metrics: MLPerfMetrics,
    plugins: Vec<Box<dyn Plugin>>,
}

impl DlioRunner {
    pub async fn run_benchmark(&mut self) -> Result<MLPerfReport> {
        // 1. Initialize plugins (including checkpoint)
        // 2. Run training loops with metric collection
        // 3. Generate MLPerf-compatible report
    }
}
```

#### **Phase 2: MLPerf Metrics Collection**
```rust
// Enhanced metrics for MLPerf compatibility
pub struct MLPerfMetrics {
    pub per_epoch_throughput: Vec<f64>,
    pub per_stage_latency: HashMap<String, Vec<Duration>>,
    pub io_latency_percentiles: Percentiles,
    pub error_counts: HashMap<String, u64>,
    pub seeded_access_order: Vec<String>, // for determinism verification
}
```

#### **Phase 3: MLPerf Report Generation**
```rust
// JSON/CSV output aligned with MLPerf reporting fields
pub struct MLPerfReport {
    pub benchmark_name: String,
    pub io_throughput_mbps: f64,
    pub latency_percentiles: LatencyPercentiles,
    pub total_samples: u64,
    pub total_time_seconds: f64,
    pub errors: u64,
    pub backend_type: String,
}
```

---

## 🎯 Implementation Tasks

### **M5 Tasks (Plugins):**
1. [ ] Create `crates/core/src/plugins/` module with Plugin trait
2. [ ] Implement `CheckpointPlugin` using s3dlio CheckpointStore
3. [ ] Add Python hooks to PyTorch and TensorFlow frameworks
4. [ ] Test checkpoint writes at N steps across all 4 backends
5. [ ] Verify compression (zstd) round-trips work
6. [ ] Integration tests with framework training loops

### **M6 Tasks (MLPerf):**
1. [ ] Create `crates/core/src/mlperf/` module with DlioRunner
2. [ ] Enhance metrics collection for per-epoch/per-stage data
3. [ ] Implement seeded access-order verification
4. [ ] Create MLPerf-compatible JSON/CSV report generation
5. [ ] Test with 2-3 stock DLIO configs (UNet3D, BERT, ResNet)
6. [ ] Validate output against MLPerf reporting requirements

### **Python Interface Requirements:**
```python
# High-level Python interface for M5 & M6
import dl_driver

# M5: Checkpoint Plugin Usage
config = dl_driver.load_dlio_config("unet3d_config.yaml")
config.checkpoint.enabled = True
config.checkpoint.steps_between_checkpoints = 100

runner = dl_driver.DlioRunner(config)
report = runner.run_benchmark()  # Includes checkpointing

# M6: MLPerf Report Generation
mlperf_report = report.to_mlperf_json()
mlperf_report.save("unet3d_benchmark_results.json")
```

---

## 🚀 Success Criteria

### **M5 Acceptance Checks:**
- ✅ YAML flag enables periodic checkpoint writes at N steps
- ✅ Output appears under the run's URI (all four backends)
- ✅ Optional compression (zstd) round-trips successfully
- ✅ Works with both PyTorch and tf.data loops (hook points defined)

### **M6 Acceptance Checks:**
- ✅ Run 2–3 stock DLIO YAMLs unmodified
- ✅ Capture per-epoch and per-stage throughput with seeded access-order parity
- ✅ Output JSON/CSV summary schema aligned with MLPerf reporting fields

### **Priority Implementation Order:**
1. **M5 CheckpointPlugin** - Leverage existing s3dlio CheckpointStore
2. **M6 MLPerf Metrics** - Enhance existing metrics collection
3. **Python Integration** - High-level interfaces for both M5 and M6
4. **End-to-End Testing** - Validate with real DLIO configs

---

## 📅 Estimated Timeline
- **M5 (Plugins)**: 2-3 days implementation + 1 day testing
- **M6 (MLPerf)**: 2-3 days implementation + 1 day validation
- **Integration & Polish**: 1-2 days

**Total**: ~1 week to complete both M5 and M6 with comprehensive testing.