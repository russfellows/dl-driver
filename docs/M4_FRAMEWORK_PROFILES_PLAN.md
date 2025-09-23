# dl-driver Framework Profiles (M4) Implementation Plan

## 🎯 Objective
Create PyTorch and TensorFlow integrations that provide native framework access to dl-driver's multi-backend, multi-format capabilities while maintaining DLIO compatibility.

## 🏗️ Architecture Design

### Core Components
1. **Framework Adapters** (`crates/frameworks/`)
   - `pytorch_adapter.rs` - PyTorch DataLoader implementation
   - `tensorflow_adapter.rs` - tf.data.Dataset implementation
   - `framework_config.rs` - Framework-specific configuration extensions

2. **Python Bindings Enhancement** (`crates/py_api/`)
   - PyTorch DataLoader Python class
   - TensorFlow Dataset Python class
   - Framework detection and optimization

3. **Integration Tests** (`tests/framework_integration/`)
   - PyTorch DataLoader tests with seed stability
   - TensorFlow Dataset tests with deterministic access
   - Cross-framework consistency validation

## 📋 Implementation Phases

### Phase 1: PyTorch DataLoader Integration
- [x] Create frameworks crate structure
- [ ] Implement PyTorchDataLoader Rust backend
- [ ] Create Python PyTorch bindings
- [ ] Add PyTorch integration tests
- [ ] Validate seed-stable access order

### Phase 2: TensorFlow Dataset Integration  
- [ ] Implement TensorFlowDataset Rust backend
- [ ] Create Python tf.data bindings
- [ ] Add TensorFlow integration tests
- [ ] Cross-validate with PyTorch implementation

### Phase 3: Configuration Extensions
- [ ] Extend DLIO config with framework profiles
- [ ] Add framework-specific optimizations
- [ ] Performance benchmarking vs DLIO
- [ ] Documentation and examples

## 🎯 Success Criteria

### PyTorch Integration
```python
from dl_driver import PyTorchDataLoader

# Use DLIO config directly
loader = PyTorchDataLoader("tests/dlio_configs/unet3d_config.yaml")
for epoch in range(10):
    for batch_idx, (data, target) in enumerate(loader):
        # Standard PyTorch training loop
        # data comes from NPZ/HDF5/TFRecord via s3dlio backends
        pass
```

### TensorFlow Integration
```python
import dl_driver.tensorflow as dldt

# Create tf.data.Dataset from DLIO config
dataset = dldt.create_dataset("tests/dlio_configs/bert_config.yaml")
for batch in dataset:
    # Standard TensorFlow training
    # Seamless integration with tf.data ecosystem
    pass
```

### Seed Stability
- Same seed produces identical batch ordering across runs
- Consistent between PyTorch and TensorFlow implementations
- Matches DLIO reference behavior

## 🚀 Next Steps
1. Create frameworks crate structure
2. Implement PyTorchDataLoader backend
3. Add Python bindings for PyTorch
4. Integration testing and validation