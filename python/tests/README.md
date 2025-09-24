# Python Integration Tests

This directory contains Python-based integration tests for dl-driver that verify compatibility with the s3dlio library and various ML frameworks.

## Test Files

### Core Verification Tests
- `test_real_io_operations.py` - **COMPREHENSIVE REAL I/O TESTING**: Performs actual file writes, S3 uploads, DirectIO operations, and data integrity verification across all backends (File, DirectIO, S3, Azure) with all ML frameworks (PyTorch, JAX, TensorFlow). This is the gold standard test that performs real network operations and byte-for-byte verification.

- `test_final_verification.py` - **BUG FIX VERIFICATION**: Verifies that s3dlio v0.8.1 accepts multi-backend URIs without the "URI must start with s3://" error. Tests URI acceptance across all backend/framework combinations.

### Additional Test Coverage
- `test_multi_backend_frameworks.py` - Multi-backend testing with actual file I/O for File and DirectIO backends, plus URI validation for S3 and Azure.

## Test Organization

- **Rust unit tests**: Located in `tests/` (crate-level testing)
- **Python integration tests**: Located in `python/tests/` (s3dlio compatibility testing)
- **CLI integration tests**: Located in `crates/cli/tests/` (command-line interface testing)

## Running Tests

```bash
# Run the comprehensive real I/O test (recommended)
python python/tests/test_real_io_operations.py

# Run the bug fix verification test
python python/tests/test_final_verification.py

# Run multi-backend framework test
python python/tests/test_multi_backend_frameworks.py
```

## Test Requirements

- **s3dlio v0.8.1+** - Multi-backend Python API
- **ML Frameworks**: PyTorch, JAX, TensorFlow (auto-detected)
- **S3 Credentials**: Real S3 server access via `.env` file
- **Azure Credentials**: Azure Blob storage access via `az login`
- **High-Capacity Storage**: Tests use `/mnt/vast1/` for large data operations

## Test Coverage

✅ **Backend Coverage**:
- File backend (buffered I/O) - `file://`
- DirectIO backend (unbuffered O_DIRECT) - `direct://`  
- S3 backend (real network operations) - `s3://`
- Azure Blob backend (multi-backend support) - `az://`

✅ **Framework Coverage**:
- PyTorch tensors and datasets
- JAX arrays and operations
- TensorFlow sequences and masks

✅ **Operation Coverage**:
- Real file system writes/reads
- Actual S3 uploads/downloads
- DirectIO unbuffered operations
- Data integrity verification (MD5 + array matching)
- Multi-backend URI acceptance
- Network error handling