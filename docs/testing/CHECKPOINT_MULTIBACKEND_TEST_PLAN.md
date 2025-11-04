# Checkpoint Multi-Backend Testing Plan

## Phase 1: Code Review ✅ COMPLETE

### Findings:
- **Architecture**: Checkpoint plugin uses `s3dlio::object_store::store_for_uri()` 
- **Backend Support**: 
  - ✅ file:// - FileSystemObjectStore (fully implemented)
  - ✅ direct:// - ConfigurableFileSystemObjectStore with O_DIRECT (fully implemented)
  - ✅ s3:// - S3ObjectStore via AWS SDK (fully implemented)
  - ✅ az:// - AzureObjectStore via Azure SDK (fully implemented)
  - ✅ gs:// - GcsObjectStore via gcloud-storage crate (fully implemented, enabled by default)

### GCS Backend Clarification:
- s3dlio has TWO GCS client implementations:
  - `gcs-community` (default): Uses `gcloud-storage` crate - **WORKS WELL**
  - `gcs-official`: Uses official `google-cloud-storage` crate - experimental
- The error message "GCS backend not yet fully implemented" found in some s3dlio factory functions is **MISLEADING**
- This error only appears in enhanced factory functions that aren't used by checkpoint plugin
- Basic `store_for_uri()` (which checkpoint plugin uses) **FULLY SUPPORTS GCS**

### Code Verification:
```rust
// dl-driver checkpoint plugin (checkpoint.rs lines 136, 383)
let store = store_for_uri(&checkpoint_uri)?;  // ✅ Supports all backends

// s3dlio factory (object_store.rs line 2267)
pub fn store_for_uri_with_logger(uri: &str, logger: Option<Logger>) -> Result<Box<dyn ObjectStore>> {
    match infer_scheme(uri) {
        Scheme::File  => FileSystemObjectStore::boxed(),
        Scheme::Direct => ConfigurableFileSystemObjectStore::boxed_direct_io(),
        Scheme::S3    => S3ObjectStore::boxed(),
        Scheme::Azure => AzureObjectStore::boxed(),
        Scheme::Gcs   => GcsObjectStore::boxed(),  // ✅ FULLY IMPLEMENTED
        Scheme::Unknown => bail!("Unable to infer backend from URI: {uri}"),
    }
}
```

### Conclusion:
**Checkpoint save/load/reload SHOULD work with all backends** because:
1. ✅ Uses `ObjectStore` trait methods exclusively (no backend-specific code)
2. ✅ Full URIs with schemes passed to all operations
3. ✅ No filesystem assumptions in checkpoint code
4. ✅ Compression/decompression is backend-agnostic
5. ✅ All backends (including GCS) implement required trait methods

---

## Phase 2: Actual Cloud Storage Testing 🔄 IN PROGRESS

### Test Matrix:

| Backend | Save Checkpoint | Load Checkpoint | Resume Training | Status |
|---------|----------------|-----------------|-----------------|--------|
| file:// | ✅ Passed | ✅ Passed | ✅ Passed | COMPLETE |
| s3://   | ⏳ Pending | ⏳ Pending | ⏳ Pending | Need credentials |
| az://   | ⏳ Pending | ⏳ Pending | ⏳ Pending | Need credentials |
| gs://   | ⏳ Pending | ⏳ Pending | ⏳ Pending | Need credentials |

### Test Files Created:
- ✅ `crates/cli/tests/checkpoint_multibackend_test.rs` - Integration tests
  - `test_file_backend_checkpoint_roundtrip()` - ✅ PASSING
  - `test_s3_backend_checkpoint_roundtrip()` - #[ignore], needs S3_TEST_BUCKET
  - `test_azure_backend_checkpoint_roundtrip()` - #[ignore], needs AZURE_TEST_CONTAINER
  - `test_mixed_backend_file_to_file()` - ✅ PASSING (simulates cross-storage migration)

### Prerequisites for Cloud Testing:

#### S3 Testing:
```bash
export AWS_ACCESS_KEY_ID="<your-key>"
export AWS_SECRET_ACCESS_KEY="<your-secret>"
export AWS_REGION="us-west-2"
export S3_TEST_BUCKET="<test-bucket-name>"

# Run test
cargo test --release --test checkpoint_multibackend_test test_s3_backend_checkpoint_roundtrip -- --ignored
```

#### Azure Testing:
```bash
export AZURE_STORAGE_ACCOUNT_NAME="<your-account>"
export AZURE_STORAGE_ACCOUNT_KEY="<your-key>"
export AZURE_TEST_CONTAINER="<test-container-name>"

# Run test
cargo test --release --test checkpoint_multibackend_test test_azure_backend_checkpoint_roundtrip -- --ignored
```

#### GCS Testing:
```bash
# GCS uses Application Default Credentials (ADC) - authenticate with:
gcloud auth application-default login

# Or use service account JSON:
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account-key.json"

# Set test bucket
export GCS_TEST_BUCKET="<test-bucket-name>"

# Need to add GCS test (not yet created)
```

### Test Scenarios to Validate:

1. **Checkpoint Save**:
   - [ ] S3: Create checkpoint on S3 bucket
   - [ ] Azure: Create checkpoint on Azure Blob Storage
   - [ ] GCS: Create checkpoint on Google Cloud Storage
   - [ ] Verify checkpoint file exists and has correct metadata

2. **Checkpoint Load**:
   - [ ] S3: Load checkpoint from S3 URI
   - [ ] Azure: Load checkpoint from Azure URI
   - [ ] GCS: Load checkpoint from GCS URI
   - [ ] Verify metadata (version, step, epoch, sizes) is correct
   - [ ] Verify zstd decompression works (if compression enabled)

3. **Resume Training**:
   - [ ] S3: Resume training from S3 checkpoint
   - [ ] Azure: Resume training from Azure checkpoint
   - [ ] GCS: Resume training from GCS checkpoint
   - [ ] Verify training starts at next epoch
   - [ ] Verify new checkpoints continue to be saved

4. **Cross-Backend Migration** (Advanced):
   - [ ] Save to file://, load from s3://
   - [ ] Save to s3://, load from az://
   - [ ] Save to az://, load from gs://

### Success Criteria:
- ✅ All checkpoint operations complete without errors
- ✅ Checkpoint metadata is accurate (uncompressed_size_bytes > 0)
- ✅ Loaded checkpoint state matches saved state
- ✅ Resume training starts at correct epoch
- ✅ No warnings in release build

### Known Limitations:
- GCS requires proper authentication setup (ADC or service account)
- Cloud storage tests require network access
- Tests marked with #[ignore] to avoid failures in CI/CD without credentials

---

## Phase 3: Documentation Updates ⏳ PENDING

After successful cloud testing:
- [ ] Update QUICK_START.md with multi-backend checkpoint examples
- [ ] Update USER_GUIDE.md checkpoint section with cloud URIs
- [ ] Document credential setup for each backend
- [ ] Add troubleshooting section for cloud storage issues

---

## Current Status: Phase 2 - Ready for Cloud Testing

**Next Steps:**
1. User provides cloud storage credentials (S3, Azure, or GCS)
2. Run ignored tests with `--ignored` flag
3. Verify all backends work correctly
4. Document any issues found
5. Proceed to Phase 3 documentation updates
