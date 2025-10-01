# Operation Log Replay Architecture

## Overview

The dl-driver replay functionality integrates with s3-bench to execute recorded operation logs against storage backends. However, replay logs contain only relative file paths, not complete storage URIs.

## Problem: Missing Storage Context

Operation logs typically contain entries like:
```json
{"operation": "GET", "file": "train_file_000001.npz", "bytes": 1024, "t_start_ns": 1000000000}
{"operation": "PUT", "file": "train_file_000002.npz", "bytes": 2048, "t_start_ns": 1500000000}
```

But for replay, we need complete URIs:
- S3: `s3://my-bucket/train_file_000001.npz`
- File: `file:///tmp/replay_test/train_file_000001.npz`
- DirectIO: `direct:///mnt/data/train_file_000001.npz`

## Solution: Base URI Remapping

We need to provide a base URI that gets prepended to relative paths from the replay log.

### CLI Enhancement
Add a `--base-uri` parameter to the replay command:

```bash
# Replay against S3
dl-driver replay --oplog ops.jsonl --base-uri s3://my-bucket/path/

# Replay against local filesystem
dl-driver replay --oplog ops.jsonl --base-uri file:///tmp/replay_test/

# Replay against DirectIO
dl-driver replay --oplog ops.jsonl --base-uri direct:///mnt/data/
```

### Path Construction Logic
1. Take relative path from replay log: `"train_file_000001.npz"`
2. Combine with base URI: `file:///tmp/replay_test/` + `train_file_000001.npz`
3. Result: `file:///tmp/replay_test/train_file_000001.npz`

### s3-bench Integration Flow

```
Operation Log → dl-driver → s3-bench
     ↓               ↓          ↓
 Relative       Add Base    Execute
  Paths          URI        Operations
```

1. **Parse Operation Log**: Read relative file paths and operations
2. **Apply Base URI**: Convert relative paths to complete URIs
3. **Apply Path Remapping**: Handle cross-environment differences  
4. **Convert to s3-bench**: Create s3-bench workload configuration
5. **Execute**: Use s3-bench workload engine for actual operations

## Implementation Strategy

### Phase 1: s3-bench Compatibility Check
- Determine if s3-bench supports file:// and direct:// URIs
- If not, implement fallback logic for local storage

### Phase 2: Base URI Implementation
- Add `--base-uri` CLI parameter
- Implement URI construction logic
- Handle trailing slash normalization

### Phase 3: Storage Backend Detection
- Auto-detect storage backend from base URI scheme
- Route to appropriate execution engine (s3-bench vs local)

## Example Configurations

### S3 Replay
```bash
dl-driver replay \
  --oplog captured_ops.jsonl \
  --base-uri s3://production-bucket/datasets/ \
  --workers 8 \
  --timeout 300
```

### Local File Replay  
```bash
dl-driver replay \
  --oplog captured_ops.jsonl \
  --base-uri file:///tmp/replay_test/ \
  --workers 4 \
  --timeout 60 \
  --fast
```

### DirectIO Replay
```bash
dl-driver replay \
  --oplog captured_ops.jsonl \
  --base-uri direct:///mnt/nvme/data/ \
  --workers 16 \
  --timeout 120
```

## Path Remapping Integration

The existing path remapping can work in combination with base URI:

1. Apply base URI: `file:///tmp/replay_test/train_file_001.npz`
2. Apply path remapping: `/tmp/replay_test/` → `/mnt/data/`
3. Result: `file:///mnt/data/train_file_001.npz`

This allows replaying logs captured in one environment against a different storage location.