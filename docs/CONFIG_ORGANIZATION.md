# Configuration File Organization

## Overview

All dl-driver configuration files are now properly organized within the project structure.

## Primary Location (Source of Truth)

**Path:** `dl-driver/tests/dlio_configs/`

This is where all dl-driver configs live and should be maintained. They are part of the project's test suite.

```
dl-driver/tests/dlio_configs/
├── DIRECTORY_MODES_README.md          # Documentation for directory structure modes
├── DISTRIBUTED_README.md              # Documentation for distributed execution
│
├── test_mode1_small_flat.yaml         # Small test: Mode 1 (Flat), 256 files, 2.5 GB
├── test_mode2_small_sharding.yaml     # Small test: Mode 2 (DLIO), 256 files, 2.5 GB
├── test_mode3_small_hierarchical.yaml # Small test: Mode 3 (Tree), 256 files, 2.5 GB
│
├── resnet50_1host_mode1_flat.yaml     # Full scale: Mode 1, 102,400 files, 100 GB
├── resnet50_1host_mode2_sharding.yaml # Full scale: Mode 2, 102,400 files, 100 GB
├── resnet50_1host_mode3_hierarchical.yaml # Full scale: Mode 3, 102,400 files, 100 GB
│
├── resnet50_1host.yaml                # CosmoFlow 1-host config
├── resnet50_4hosts.yaml               # CosmoFlow 4-host distributed config
├── resnet50_8hosts.yaml               # CosmoFlow 8-host distributed config
│
├── unet3d_1host.yaml                  # 3D U-Net 1-host config
├── unet3d_4hosts.yaml                 # 3D U-Net 4-host config
├── unet3d_8hosts.yaml                 # 3D U-Net 8-host config
│
├── distributed_2node_local.yaml       # Distributed: 2-node local filesystem
├── distributed_2node_gcs.yaml         # Distributed: 2-node Google Cloud Storage
├── distributed_4node_local.yaml       # Distributed: 4-node local filesystem
├── distributed_4node_gcs.yaml         # Distributed: 4-node GCS with checkpointing
│
└── (other existing test configs...)
```

## Secondary Location (Container Examples)

**Path:** `Containers/sai3-multi/configs/AI-ML/dl-driver/`

This is a **copy** of the configs for use in container-based testing and multi-tool environments. Files here should be synced from the primary location.

### Directory Structure Separation

```
Containers/sai3-multi/configs/
├── AI-ML/
│   ├── dl-driver/           # dl-driver YAML configs (DLIO format)
│   │   ├── test_mode1_small_flat.yaml
│   │   ├── test_mode2_small_sharding.yaml
│   │   └── ...
│   │
│   └── (future: other AI/ML tools)
│
└── Basic/
    └── (sai3-bench YAML configs - different format!)
```

**Important:** sai3-bench and dl-driver use **different config formats**:
- **sai3-bench**: Custom YAML schema (operations, workload mix, prepare phase)
- **dl-driver**: DLIO-compatible YAML schema (dataset, reader, train, workflow)

## Usage Examples

### From dl-driver Project Directory

```bash
cd /home/eval/Documents/Code/dl-driver

# Validate config
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml --dry-run

# Run test
./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml
```

### From Container Environment

```bash
cd /home/eval/Documents/Code/Containers/sai3-multi

# Use copied configs in container testing
docker run -v $(pwd)/configs:/configs dl-driver-image \
  dl-driver run --config /configs/AI-ML/dl-driver/test_mode1_small_flat.yaml
```

## Synchronization

When updating configs, follow this workflow:

1. **Edit in primary location:** `dl-driver/tests/dlio_configs/`
2. **Test from dl-driver project:** `./target/release/dl-driver run --config tests/dlio_configs/...`
3. **Copy to container location:** `cp dl-driver/tests/dlio_configs/*.yaml Containers/sai3-multi/configs/AI-ML/dl-driver/`
4. **Commit both locations**

Or use this helper script:

```bash
# From workspace root
cp dl-driver/tests/dlio_configs/*.yaml Containers/sai3-multi/configs/AI-ML/dl-driver/
```

## Documentation Files

- `tests/dlio_configs/DIRECTORY_MODES_README.md` - Comprehensive guide to Mode 1/2/3 directory structures
- `tests/dlio_configs/DISTRIBUTED_README.md` - Multi-host distributed execution guide
- `docs/DRY_RUN_FEATURE.md` - --dry-run flag documentation

## Quick Reference

### Small Tests (2.5 GB each)
```bash
cd /home/eval/Documents/Code/dl-driver

./target/release/dl-driver run --config tests/dlio_configs/test_mode1_small_flat.yaml
./target/release/dl-driver run --config tests/dlio_configs/test_mode2_small_sharding.yaml
./target/release/dl-driver run --config tests/dlio_configs/test_mode3_small_hierarchical.yaml
```

### Full-Scale Tests (100 GB each)
```bash
./target/release/dl-driver run --config tests/dlio_configs/resnet50_1host_mode1_flat.yaml
./target/release/dl-driver run --config tests/dlio_configs/resnet50_1host_mode2_sharding.yaml
./target/release/dl-driver run --config tests/dlio_configs/resnet50_1host_mode3_hierarchical.yaml
```

### Distributed Tests
```bash
# Start agents first
./target/release/dl_driver_agent --agent-id agent-0 --listen 127.0.0.1:50051
./target/release/dl_driver_agent --agent-id agent-1 --listen 127.0.0.1:50052

# Run distributed workload
./target/release/dl-driver distributed run \
  --config tests/dlio_configs/distributed_2node_local.yaml \
  --agents http://127.0.0.1:50051,http://127.0.0.1:50052 \
  --path-template "{id}/"
```

## Summary

- ✅ **Primary configs:** `dl-driver/tests/dlio_configs/` (6 new mode configs + existing)
- ✅ **Container copies:** `Containers/sai3-multi/configs/AI-ML/dl-driver/` (for multi-tool testing)
- ✅ **Documentation:** 3 comprehensive README files explaining usage
- ✅ **All paths updated:** DRY_RUN_FEATURE.md uses correct project-relative paths
