# Distributed Execution Example Configs

This directory contains example configurations for testing distributed DLIO workloads.

## Available Configurations

### 2-Node Configurations

- **`distributed_2node_local.yaml`** - Local filesystem storage with agent-specific paths
- **`distributed_2node_gcs.yaml`** - Google Cloud Storage with shared bucket

### 4-Node Configurations

- **`distributed_4node_local.yaml`** - Local filesystem with checkpointing
- **`distributed_4node_gcs.yaml`** - GCS with larger-scale workload and checkpointing

## Quick Start

### 1. Start Agent Processes

Each agent runs as a separate process. For 2-node testing:

```bash
# Terminal 1 - Agent 0
./target/release/dl_driver_agent --agent-id agent-0 --listen 127.0.0.1:50051

# Terminal 2 - Agent 1
./target/release/dl_driver_agent --agent-id agent-1 --listen 127.0.0.1:50052
```

For 4-node testing, start agents on ports 50051-50054.

### 2. Run Distributed Workload

#### Local Storage Example

```bash
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_local.yaml \
  --agents http://127.0.0.1:50051,http://127.0.0.1:50052 \
  --path-template "{id}/"
```

**Note:** Local storage requires `--path-template` to append agent IDs to the base path, creating isolated subdirectories for each agent. For example, with base path `/tmp/dl-dist` and template `{id}/`, agents will use `/tmp/dl-dist/agent-0/`, `/tmp/dl-dist/agent-1/`, etc.

#### Google Cloud Storage Example

```bash
# First, ensure GCS authentication is set up:
# export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account-key.json
# or use: gcloud auth application-default login

# Update the bucket name in the config file:
sed -i 's/your-dlio-test-bucket/YOUR_ACTUAL_BUCKET/g' tests/dlio_configs/distributed_2node_gcs.yaml

./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_gcs.yaml \
  --agents http://127.0.0.1:50051,http://127.0.0.1:50052
```

**Note:** GCS is shared storage, so no `--path-template` is needed.

### 3. Collect Results

The controller will output:
- Individual agent metrics (saved to separate TSV files)
- Aggregate metrics across all agents
- Storage metrics TSV: `distributed_storage_metrics.tsv`
- AI/ML metrics TSV: `distributed_aiml_metrics.tsv`

## Storage Backend Behavior

### Local Storage (`file://`)
- **Path Template Required:** Yes (`--path-template "{id}/"`)
- **Behavior:** Each agent works in isolated subdirectory appended to base path
- **Example:** With base `/tmp/dl-dist` and template `{id}/`:
  - Agent 0 → `/tmp/dl-dist/agent-0/`
  - Agent 1 → `/tmp/dl-dist/agent-1/`
- **Use Case:** Testing on single machine, simulating distributed filesystem

### Google Cloud Storage (`gs://`)
- **Path Template Required:** No
- **Behavior:** All agents access shared bucket
- **Example:** All agents read/write to `gs://bucket/distributed-test/`
- **Use Case:** Real cloud deployment, shared training data
- **Setup:** 
  1. Replace `<YOUR-GCS-BUCKET>` in GCS config files with your actual bucket name
  2. Authenticate: `gcloud auth application-default login`
  3. Ensure bucket exists and you have read/write permissions

## Environment Variables

### Google Cloud Storage
```bash
# Service account key
export GOOGLE_APPLICATION_CREDENTIALS=/path/to/key.json

# Or use gcloud CLI
gcloud auth application-default login
```

### Agent Configuration
```bash
# Optional: Set custom log level for agents
export RUST_LOG=info  # or debug, trace
```

## Customization

### Adjust Workload Size

Edit the YAML files to modify:
- `dataset.num_files_train` - Number of files to generate/train on
- `dataset.record_length_bytes` - Size of each data record
- `reader.batch_size` - Batch size for training
- `train.epochs` - Number of training epochs

### Adjust Coordination

Use CLI flags:
- `--start-delay <ms>` - Milliseconds to wait before coordinated start (default: 1000)
- `--timeout <ms>` - Request timeout in milliseconds (default: 300000)
- `--dry-run` - Validate configuration without executing

## Troubleshooting

### Agents not connecting
- Verify agents are running: `ps aux | grep dl_driver_agent`
- Check firewall rules for ports 50051-50054
- Try: `curl http://127.0.0.1:50051` (should get connection)

### GCS authentication errors
- Verify credentials: `gcloud auth application-default print-access-token`
- Check bucket exists: `gsutil ls gs://your-bucket-name`
- Verify bucket permissions: Read/Write required

### Path conflicts (local storage)
- Ensure `--path-template "agent-{id}/"` is specified
- Clean up old data: `rm -rf /tmp/dlio_distributed_*`

## Performance Notes

### Local Storage
- Limited by local disk I/O
- Good for: Functional testing, development
- Typical throughput: 500-2000 MB/s (SSD)

### Google Cloud Storage
- Limited by network bandwidth
- Good for: Production testing, benchmarking
- Typical throughput: 100-1000 MB/s (depends on instance type)

## Next Steps

After validating with these example configs:
1. Create production configs with your actual bucket names
2. Test with real multi-host deployment
3. Scale to larger node counts (8, 16, 32 nodes)
4. Monitor with metrics TSV outputs for performance analysis
