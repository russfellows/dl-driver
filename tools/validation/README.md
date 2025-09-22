# Format Validation Tools

This directory contains tools for validating dl-driver's format compatibility with standard Python libraries.

## Files

### `validate_formats.py`
Comprehensive validation script that tests all supported formats (NPZ, HDF5, TFRecord) for compatibility with:
- NumPy (for NPZ files)
- h5py (for HDF5 files) 
- TensorFlow (for TFRecord files)

**Usage:**
```bash
# Validate all formats
uv run validate_formats.py

# Validate specific format
uv run validate_formats.py --format tfrecord --path /path/to/file.tfrecord
```

### `test_configs/`
Configuration files for format-specific validation tests:
- `npz_validation_config.yaml` - NPZ format test configuration
- `hdf5_validation_config.yaml` - HDF5 format test configuration
- `tfrecord_validation_config.yaml` - TFRecord format test configuration

### `create_reference_tfrecord.py`
Creates reference TFRecord files using TensorFlow's Python API for comparison with dl-driver generated files.

### `debug_crc.py`
Utility for debugging CRC differences between reference and dl-driver generated TFRecord files.

## Requirements

Install Python dependencies:
```bash
uv pip install numpy h5py tensorflow crc32c
```

## Validation Framework

The validation framework ensures that dl-driver generates format-compliant files by:

1. **Generating test data** using dl-driver
2. **Manual parsing** to verify file structure
3. **Library validation** using standard Python libraries
4. **Cross-validation** between manual parsing and library results

This comprehensive approach ensures 100% compatibility with the Python ML ecosystem.