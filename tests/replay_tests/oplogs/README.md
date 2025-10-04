# Test Operation Logs

This directory contains test operation logs for streaming replay tests.

## Generating Test Files

The test op-log files are **not committed** to the repository. They must be generated before running tests.

To generate all test op-log files:

```bash
./scripts/generate_test_oplogs.sh
```

This will create:
- `sample_file_backend.csv.zst` - File backend operations (10 ops)
- `sample_s3_backend.csv.zst` - S3 backend operations (9 ops)
- `sample_azure_backend.csv.zst` - Azure Blob operations (7 ops)
- `sample_gcs_backend.csv.zst` - GCS operations (9 ops)
- `sample_directio_backend.csv.zst` - DirectIO operations (7 ops)
- `real_directio_test.csv.zst` - Real DirectIO test (4 ops)
- `real_s3_test.csv.zst` - Real S3 test (5 ops)

## File Format

All op-logs use the s3dlio-oplog TSV format:

```tsv
idx	op	bytes	endpoint	file	start	duration_ns	error
1	PUT	1048576	s3://bucket	path/file.bin	2025-10-03T10:00:00Z	50000000	
```

Files are compressed with zstd and use the `.csv.zst` extension for compatibility with s3dlio-oplog's `OpLogStreamReader`.

## Requirements

- `zstd` command-line tool must be installed
- Run `./scripts/generate_test_oplogs.sh` before running tests

## CI/CD

In CI pipelines, add this step before running tests:

```bash
./scripts/generate_test_oplogs.sh
```
