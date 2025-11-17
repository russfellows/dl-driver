# dl-driver Project Instructions

## ⚠️ CRITICAL: Build and Test Commands

**NEVER pipe build or test commands through head/tail/grep**

❌ **WRONG:** `cargo build 2>&1 | tail -50`  
✅ **CORRECT:** `cargo build --release` (see ALL output)

User has explicitly stated this requirement 100+ times. Piping obscures errors and warnings.

This project is a high-performance, MLCommons DLIO-compatible data loading framework in Rust, designed for unified access to multiple storage backends (File, S3, Azure, DirectIO) via the s3dlio library. It is structured for drop-in replacement of DLIO benchmarks and enterprise-scale workloads.

## s3dlio Dependency

This project **heavily leverages the s3dlio project**, which is located in a parallel directory at `/home/eval/Documents/Code/s3dlio`. The s3dlio library provides the core ObjectStore abstraction and backend implementations that dl-driver builds upon. When debugging storage-related issues or understanding the data flow, you may need to reference or modify code in the s3dlio project as well.

## s3-bench Integration

This project **integrates with s3-bench** (https://github.com/russfellows/s3-bench) as a GitHub dependency for operation log replay functionality. Rather than maintaining duplicate replay logic, dl-driver delegates to s3-bench's existing workload engine:

- **Replay Architecture**: `S3BenchReplayEngine` converts operation logs to s3-bench workload configurations
- **API Integration**: Uses `s3_bench::workload::run()` with `s3_bench::config::Config` structures
- **Scope Separation**: s3-bench handles replay/benchmarking, dl-driver focuses on DLIO compatibility and AI/ML patterns

## Key Architecture & Patterns

- **Workspace:** 5 Rust crates in `crates/`:
    - `core/`: config parsing, workload orchestration, metrics
    - `storage/`: legacy POSIX backend (use s3dlio's ObjectStore for new code)
    - `formats/`: data format handlers (NPZ, HDF5, etc.)
    - `cli/`: main binary, CLI parsing
    - `py_api/`: Python bindings (PyO3)
- **Primary data path:** All storage I/O uses s3dlio's `ObjectStore` trait. Always construct object stores via s3dlio, not the legacy storage crate.
- **Config system:** Supports both MLCommons DLIO YAML and legacy configs. Use `DlioConfig::from_yaml_file()` for DLIO, `Config::from_yaml_file()` for legacy. CLI: `dl-driver dlio|legacy --config ...`.
- **Backend detection:** Storage backend is auto-detected from the `data_folder` URI scheme (e.g., `s3://`, `az://`, `direct://`, `file://`).
- **Workload execution:** Orchestrated by `WorkloadRunner` (`core/src/workload.rs`), which manages async I/O, metrics, and three-phase workflow (data gen, train, checkpoint).
- **DLIO integration:** All DLIO config fields mapped in `core/src/dlio_compat.rs`. Automatic conversion to s3dlio LoaderOptions/PoolConfig.
- **Replay integration:** Operation log replay via `S3BenchReplayEngine` in `core/src/replay.rs`, delegates to s3-bench workload engine.
- **Testing:** All tests are config-driven (YAML in `tests/configs/`). S3/Azure tests skip if credentials are missing. MLCommons config validation in `tests/mlcommons_dlio_validation.rs`.

## Critical Workflows & Commands

- **Build:** `cargo build --release` (ALWAYS show full output - NEVER pipe to head/tail/grep unless specifically searching for strings)
- **Test:** `cargo test` (ALWAYS show full output - NEVER pipe to head/tail unless specifically searching for strings)
- **Run workload:**
    - Legacy: `./target/release/dl-driver legacy --config tests/configs/test_file_config.yaml`
    - DLIO: `./target/release/dl-driver dlio --config tests/dlio_configs/minimal_config.yaml`
- **Validate config:** `./target/release/dl-driver validate --config tests/dlio_configs/unet3d_config.yaml`
- **Replay operations:** `./target/release/dl-driver replay --oplog path/to/log.jsonl --workers 8 --timeout 300`
- **MLCommons validation:** `cargo test --test mlcommons_dlio_validation`

### Important Notes on Command Output:
- **ALWAYS show full cargo build output** - don't use `| head` or `| tail`
- **ALWAYS show full cargo test output** - don't use `| head` or `| tail`
- **Exception**: When specifically searching for patterns, `| grep` is acceptable (e.g., `cargo test 2>&1 | grep "test result:"`)
- **Rationale**: We may miss critical warnings, errors, or context if output is truncated
- Use `2>&1` to capture both stdout and stderr when needed
- When there are compilation errors or warnings, full context is critical

## Project-Specific Conventions

- **Large data:** Never use `/tmp/` for >1GB data. Use `/mnt/test/` for large test data and update configs accordingly.
- **Error handling:** Use `anyhow::Result` and `.context()` for error chains. Storage errors bubble up from s3dlio.
- **Async:** All main execution is async (`#[tokio::main]`), all I/O via s3dlio is async. Tests use `#[tokio::test]`.
- **Naming:** Configs: `test_[backend]_config.yaml`. Generated files: `train_file_{:06}.{format}`. Metrics: files_processed, bytes_read/written, execution times.
- **Performance:** Use s3dlio's PoolConfig for concurrent I/O. Prefer streaming to loading whole files. Use DirectIO for HPC, async pools for cloud.
- **Dependencies:** Use s3-bench for replay/benchmarking functionality. Do not reimplement workload patterns that s3-bench already provides.

## Integration & Troubleshooting

- **Credentials:** S3 via `.env` (dotenvy), Azure via `AZURE_BLOB_ACCOUNT`/`AZURE_BLOB_CONTAINER` env vars. Credential loading is in `WorkloadRunner`.
- **Path resolution:** Always use full URIs, handle trailing slashes.
- **s3dlio dependency:** Do not reimplement storage logic; always use s3dlio for new storage code.
- **s3-bench dependency:** Do not reimplement replay logic; use `S3BenchReplayEngine` and s3-bench workload configurations.
- **Format support:** NPZ is primary; HDF5 is planned but not fully implemented.
- **Test skipping:** Tests skip (not fail) if credentials are missing.

## Examples

**Storage backend pattern:**
```rust
match config.storage_backend() {
        StorageBackend::S3 => /* S3 credentials */
        StorageBackend::Azure => /* Azure env vars */
        StorageBackend::DirectIO => /* O_DIRECT optimizations */
        StorageBackend::File => /* Standard filesystem */
}
```

**s3dlio object store usage:**
```rust
let store = store_for_uri(&config.storage_uri()).await?;
let loader = AsyncPoolDataLoader::new(dataset, pool_config).await?;
```

**s3-bench replay integration:**
```rust
let engine = S3BenchReplayEngine::new(config);
let stats = engine.run().await?;
```

**Test config location:** See `tests/dlio_configs/` for MLCommons configs (e.g., `minimal_config.yaml`, `unet3d_config.yaml`).

---
If any section is unclear or missing, please provide feedback for further refinement.