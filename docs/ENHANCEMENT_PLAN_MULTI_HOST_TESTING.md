# Enhancement Plan: Multi-Host Testing Capabilities for dl-driver

**Date**: October 22, 2025  
**Version**: Draft v1.0  
**Author**: AI Assistant  
**Reference**: Based on sai3-bench v0.6.11+ distributed testing architecture

## Executive Summary

This document outlines a comprehensive plan to add multi-host distributed testing capabilities to dl-driver, modeled after the successful implementation in sai3-bench. The goal is to enable large-scale AI/ML workload testing across multiple hosts while maintaining dl-driver's focus on DLIO compatibility and AI/ML-specific patterns.

## Current State (v0.8.1)

### ✅ What We Have
- **Basic distributed controller** (v0.8.0): Multi-agent orchestration via gRPC
- **Agent binary**: `dl_driver_agent` with health checks and coordinated start
- **Path isolation**: Agent-specific prefixes for local storage
- **Shared storage detection**: Automatic detection of S3/GCS/Azure
- **Dual metrics**: Storage and AI/ML perspectives
- **s3dlio v0.9.10**: Latest performance optimizations

### ⚠️ What We're Missing (Compared to sai3-bench)
1. **SSH-automated deployment**: No automatic agent deployment across VMs
2. **Container deployment**: No Docker/Podman integration
3. **Scale-up support**: No multi-agent on single host pattern
4. **Advanced results collection**: No inline results transfer via gRPC
5. **Per-agent target overrides**: No ability to test multiple backends simultaneously
6. **Region-specific testing**: No multi-region latency testing patterns
7. **Comprehensive docs**: Need deployment guides like sai3-bench

## Gap Analysis

### Architecture Comparison

| Feature | sai3-bench v0.6.11+ | dl-driver v0.8.1 | Gap |
|---------|---------------------|------------------|-----|
| **Agent/Controller Pattern** | ✅ | ✅ | ✅ Equal |
| **gRPC Protocol** | ✅ | ✅ | ✅ Equal |
| **Health Checks** | ✅ | ✅ | ✅ Equal |
| **Coordinated Start** | ✅ | ✅ | ✅ Equal |
| **Path Isolation** | ✅ | ✅ | ✅ Equal |
| **SSH Deployment** | ✅ | ❌ | ⚠️ MISSING |
| **Container Support** | ✅ (Docker/Podman) | ❌ | ⚠️ MISSING |
| **Scale-Up Pattern** | ✅ (multi-port) | ❌ | ⚠️ MISSING |
| **Inline Results** | ✅ (via gRPC) | ❌ | ⚠️ MISSING |
| **Target Overrides** | ✅ (per-agent) | ❌ | ⚠️ MISSING |
| **HDR Histogram Transfer** | ✅ | ❌ | ⚠️ MISSING |
| **Deployment Docs** | ✅ Comprehensive | ⚠️ Basic | ⚠️ NEEDS WORK |

### Workload Differences

| Aspect | sai3-bench | dl-driver | Convergence Strategy |
|--------|------------|-----------|---------------------|
| **Primary Use Case** | Generic I/O benchmarking | AI/ML training workloads | Keep AI/ML focus, add flexibility |
| **Workload Definition** | Op weights (get/put/list) | Epochs, batches, compute time | Add op-level patterns |
| **Data Generation** | Generic objects | NPZ/HDF5/TFRecord format | Keep format support |
| **Metrics** | Storage-centric | Dual (storage + AI/ML) | Keep dual metrics |
| **Config Format** | Custom YAML | DLIO-compatible YAML | Keep DLIO compatibility |

## Enhancement Plan

### Phase 1: Core Infrastructure (v0.9.0) - 2-3 weeks

#### 1.1 Enhanced Proto Definition

**Goal**: Extend `bench.proto` to match sai3-bench capabilities

**Changes**:
```protobuf
// Extend WorkloadSummary with inline results (like sai3-bench)
message WorkloadSummary {
  // ... existing fields ...
  
  // NEW: Inline results collection
  string console_log = 20;          // Agent console output
  string metadata_json = 21;        // Agent metadata JSON
  string storage_tsv_content = 22;  // Storage metrics TSV
  string aiml_tsv_content = 23;     // AI/ML metrics TSV
  string results_path = 24;         // Local path where agent saved results
  
  // NEW: HDR histogram data for accurate aggregation
  bytes histogram_read = 25;        // Serialized read latency histogram
  bytes histogram_write = 26;       // Serialized write latency histogram
  bytes histogram_batch = 27;       // Serialized batch timing histogram
}

// NEW: Per-agent configuration overrides
message AgentConfig {
  string agent_id = 1;
  string target_override = 2;       // Override data_folder for this agent
  string path_prefix = 3;           // Path isolation prefix
  map<string, string> env_vars = 4; // Environment variable overrides
}

message RunWorkloadRequest {
  // ... existing fields ...
  
  // NEW: Per-agent overrides
  AgentConfig agent_config = 10;
  
  // NEW: Multi-backend testing
  bool shared_storage = 11;
}
```

**Implementation**:
- Update `proto/bench.proto`
- Regenerate Rust code via `build.rs`
- Update `crates/core/src/dist/types.rs` with new fields
- Update `crates/core/src/dist/agent.rs` to populate inline results

**Testing**:
- Unit tests for proto serialization
- Integration tests for inline results transfer

#### 1.2 HDR Histogram Integration

**Goal**: Accurate histogram merging across agents (like sai3-bench)

**Current Issue**: We aggregate percentiles by averaging, which is mathematically incorrect.

**Solution**: Use HDR histogram library for proper histogram merging.

**Dependencies**:
```toml
# Add to crates/core/Cargo.toml
hdrhistogram = "7.5"
```

**Implementation**:
- Add `histogram` field to `WorkloadMetrics`
- Serialize/deserialize HDR histograms to/from bytes
- Update `AggregateResults` to merge histograms, not percentiles
- Update TSV output with merged histogram percentiles

**Benefits**:
- Mathematically correct p50/p90/p95/p99 across all agents
- Matches sai3-bench accuracy
- Better statistical analysis capabilities

#### 1.3 Per-Agent Configuration Overrides

**Goal**: Allow each agent to target different storage backends

**Use Cases**:
- Multi-cloud testing (AWS agent → S3, Azure agent → Blob, GCP agent → GCS)
- Cross-region latency testing (US agent → US bucket, EU agent → EU bucket)
- Backend comparison (same data, different backends)

**Config Example**:
```yaml
# Base config applies to all agents
dataset:
  data_folder: "s3://default-bucket/data/"  # Fallback
  format: npz

# Per-agent overrides
distributed:
  agents:
    - address: "aws-vm:50051"
      id: "agent-aws"
      target_override: "s3://us-east-1-bucket/data/"
      env:
        AWS_REGION: "us-east-1"
    
    - address: "azure-vm:50051"
      id: "agent-azure"
      target_override: "az://eastus-storage/data/"
      env:
        AZURE_STORAGE_ACCOUNT: "eastus-storage"
    
    - address: "gcp-vm:50051"
      id: "agent-gcp"
      target_override: "gs://us-central1-bucket/data/"
      env:
        GOOGLE_CLOUD_PROJECT: "my-project"
```

**Implementation**:
- Add `target_override` and `env_vars` to `DistributedAgentConfig`
- Update controller to send per-agent config in `RunWorkloadRequest`
- Update agent to apply overrides before workload execution
- Update path utilities to handle per-agent targets

### Phase 2: Deployment Automation (v0.9.1) - 2-3 weeks

#### 2.1 SSH Deployment Module

**Goal**: Automatic agent deployment via SSH (like sai3-bench)

**New Files**:
- `crates/core/src/dist/ssh_deploy.rs` - SSH deployment logic
- `crates/core/src/dist/ssh_setup.rs` - SSH key setup utilities

**New CLI Commands**:
```bash
# Setup SSH access to remote hosts
dl-driver ssh-setup \
  --hosts vm1.cloud.com,vm2.cloud.com \
  --user ubuntu \
  --ssh-key ~/.ssh/id_rsa

# Deploy agents to remote hosts
dl-driver distributed deploy \
  --hosts vm1.cloud.com,vm2.cloud.com \
  --binary ./target/release/dl_driver_agent \
  --port 50051

# Run workload with auto-deployment
dl-driver distributed run \
  --config my-workload.yaml \
  --hosts vm1.cloud.com,vm2.cloud.com \
  --deploy-ssh
```

**Implementation**:
- Use `ssh2` crate for SSH operations
- SCP binary upload to remote hosts
- Remote execution of agent binary
- Automatic cleanup after workload completion

**Dependencies**:
```toml
# Add to crates/core/Cargo.toml
ssh2 = "0.9"
```

#### 2.2 Container Deployment Support

**Goal**: Deploy agents in Docker/Podman containers

**New Files**:
- `crates/core/src/dist/container_deploy.rs`
- `Dockerfile` at project root

**Dockerfile Example**:
```dockerfile
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy dl-driver binaries
COPY target/release/dl-driver /usr/local/bin/
COPY target/release/dl_driver_agent /usr/local/bin/

WORKDIR /workspace
CMD ["/bin/bash"]
```

**New CLI Commands**:
```bash
# Build container image
docker build -t dl-driver:0.9.0 .

# Deploy agents in containers
dl-driver distributed deploy \
  --hosts vm1.cloud.com,vm2.cloud.com \
  --container docker \
  --image dl-driver:0.9.0 \
  --port 50051

# Run workload with container deployment
dl-driver distributed run \
  --config my-workload.yaml \
  --hosts vm1.cloud.com,vm2.cloud.com \
  --deploy-container docker
```

**Implementation**:
- SSH to each host
- Execute `docker run` with proper environment variables
- Map credentials from host to container
- Automatic container cleanup

#### 2.3 Scale-Up Pattern (Multi-Agent on Single Host)

**Goal**: Run multiple agents on one powerful VM (like sai3-bench)

**Config Example**:
```yaml
distributed:
  agents:
    - address: "big-vm.cloud.com:50051"
      id: "agent-1"
    - address: "big-vm.cloud.com:50052"
      id: "agent-2"
    - address: "big-vm.cloud.com:50053"
      id: "agent-3"
    - address: "big-vm.cloud.com:50054"
      id: "agent-4"
  
  deployment:
    mode: "scale-up"  # Multiple agents on same host
    container_runtime: "docker"
```

**CLI Command**:
```bash
dl-driver distributed run \
  --config scale-up-config.yaml \
  --host big-vm.cloud.com \
  --agents-per-host 4 \
  --base-port 50051
```

**Implementation**:
- Automatically assign sequential ports (50051, 50052, 50053, 50054)
- Launch multiple agent containers on same host
- Each agent gets unique ID and path prefix
- Controller connects to all ports on same host

### Phase 3: Documentation & Examples (v0.9.2) - 1 week

#### 3.1 New Documentation Files

**Create**:
- `docs/DISTRIBUTED_TESTING_GUIDE.md` - Comprehensive guide (like sai3-bench)
- `docs/CONTAINER_DEPLOYMENT_GUIDE.md` - Container deployment patterns
- `docs/SSH_SETUP_GUIDE.md` - SSH configuration and troubleshooting
- `docs/SCALE_OUT_VS_SCALE_UP.md` - Deployment strategy comparison
- `docs/MULTI_CLOUD_TESTING.md` - Cross-cloud testing patterns

**Content Structure** (based on sai3-bench docs):
1. Quick Start (4-host example)
2. Common Patterns (shared storage, independent storage, scaling)
3. Cloud Provider Setup (AWS, Azure, GCP)
4. Advanced Patterns (multi-cloud, multi-region, scale-up)
5. Troubleshooting
6. Best Practices

#### 3.2 Example Configurations

**Create** in `tests/dlio_configs/`:
- `distributed_multicloud_npz.yaml` - NPZ format across AWS/Azure/GCP
- `distributed_scale_up_4agents.yaml` - 4 agents on 1 VM
- `distributed_cross_region_latency.yaml` - Multi-region testing
- `distributed_directio_local.yaml` - DirectIO performance testing

#### 3.3 Deployment Scripts

**Create** in `scripts/`:
- `deploy_multi_host.sh` - Example multi-host deployment
- `run_cross_cloud_test.sh` - Multi-cloud testing script
- `cleanup_agents.sh` - Cleanup script for distributed agents

### Phase 4: s3dlio Feature Integration (v0.9.3) - 1 week

#### 4.1 ObjectSizeCache Integration

**Goal**: Use s3dlio v0.9.10's `pre_stat_and_cache()` for training epochs

**Current Bottleneck**: Training loops re-stat files every epoch

**Solution**: Pre-stat all training files once before first epoch

**Implementation** in `crates/core/src/workload.rs`:
```rust
// Before training loop
if let Some(store) = &self.object_store {
    // Get list of all training files
    let training_files: Vec<String> = self.get_training_file_list()?;
    
    // Pre-stat and cache all file sizes (NEW in v0.9.10)
    info!("Pre-stating {} training files for cache...", training_files.len());
    let cached_count = store.pre_stat_and_cache(&training_files, 100).await?;
    info!("Cached {} file sizes (99% stat overhead eliminated)", cached_count);
}

// Training loop now benefits from cached sizes
for epoch in 1..=epochs {
    for batch in batches {
        let data = store.get(file_uri).await?;  // Uses cached size!
        // ... process batch
    }
}
```

**Expected Benefit**: 2.5x faster for workloads with many files (1000+)

#### 4.2 PageCacheMode Configuration

**Goal**: Expose s3dlio v0.9.8's configurable page cache hints

**Config Addition**:
```yaml
dataset:
  data_folder: "file:///nvme/training-data/"
  format: npz
  
  # NEW: Page cache optimization (file:// backend only)
  page_cache_mode: "sequential"  # Options: auto, sequential, random, dont_need
```

**Implementation**:
- Add `page_cache_mode` field to `DlioConfig`
- Pass to `FileSystemConfig` when creating object store
- Document in USER_GUIDE.md

**Use Cases**:
- `sequential`: Large sequential training file reads
- `random`: Sparse random access to large datasets
- `dont_need`: One-time reads (don't pollute cache)

#### 4.3 DirectIO Buffer Pool (Already Automatic)

**Status**: ✅ Already enabled in s3dlio v0.9.9

**Benefit**: Automatic 15-20% throughput gain for `direct://` URIs

**Documentation**: Add performance notes to docs

## Implementation Timeline

### Sprint 1 (Week 1-2): Phase 1 Core Infrastructure
- Enhanced proto definition
- HDR histogram integration
- Per-agent configuration overrides
- Testing and validation

### Sprint 2 (Week 3-4): Phase 2 Deployment (Part 1)
- SSH deployment module
- SSH setup utilities
- CLI commands for ssh-deploy
- Basic testing

### Sprint 3 (Week 5-6): Phase 2 Deployment (Part 2)
- Container deployment support
- Dockerfile and container build
- Scale-up pattern implementation
- End-to-end testing

### Sprint 4 (Week 7): Phase 3 Documentation
- Comprehensive deployment guides
- Example configurations
- Deployment scripts
- User documentation updates

### Sprint 5 (Week 8): Phase 4 s3dlio Integration
- ObjectSizeCache integration
- PageCacheMode configuration
- Performance testing and validation
- Release preparation

**Total Timeline**: ~8 weeks to v0.9.3

## Success Criteria

### Functional Requirements
- ✅ SSH-automated agent deployment works
- ✅ Container deployment (Docker/Podman) works
- ✅ Scale-up pattern (4+ agents on 1 VM) works
- ✅ Per-agent target overrides work
- ✅ HDR histogram aggregation is mathematically correct
- ✅ Inline results collection via gRPC works
- ✅ ObjectSizeCache integration provides 2x+ speedup

### Performance Requirements
- ✅ 10+ agent deployment completes in < 30 seconds
- ✅ Workload coordination overhead < 1%
- ✅ Results aggregation completes in < 5 seconds
- ✅ Training epoch performance matches single-node

### Documentation Requirements
- ✅ Deployment guides cover all patterns
- ✅ Example configs for all use cases
- ✅ Troubleshooting guide is comprehensive
- ✅ Migration guide from v0.8.x to v0.9.x

## Risks and Mitigation

### Risk 1: SSH Complexity
**Risk**: SSH deployment may fail across diverse environments  
**Mitigation**: Comprehensive SSH troubleshooting guide, fallback to manual agent startup

### Risk 2: Container Compatibility
**Risk**: Docker/Podman differences may cause issues  
**Mitigation**: Test both runtimes, document differences, provide examples

### Risk 3: HDR Histogram Overhead
**Risk**: Histogram serialization may be slow/large  
**Mitigation**: Use compressed serialization, test with large agent counts

### Risk 4: Breaking Changes
**Risk**: Proto changes may break existing deployments  
**Mitigation**: Maintain backward compatibility, version negotiation

## Future Enhancements (Post v0.9.3)

### v0.10.0: Kubernetes Integration
- Helm charts for agent deployment
- Kubernetes operator for workload orchestration
- Auto-scaling based on workload size

### v0.10.1: Advanced Scheduling
- Agent affinity/anti-affinity rules
- Resource-aware scheduling
- Cost optimization for cloud deployments

### v0.10.2: Real-Time Monitoring
- Grafana dashboards
- Prometheus metrics export
- Live progress visualization

## Comparison: sai3-bench vs dl-driver (Post-Enhancement)

| Feature | sai3-bench v0.6.11 | dl-driver v0.9.3 (planned) |
|---------|-------------------|----------------------------|
| **Use Case** | Generic I/O benchmarking | AI/ML training workloads |
| **Config Format** | Custom YAML | DLIO-compatible YAML |
| **Workload Model** | Op-based (get/put/list) | Epoch/batch-based + ops |
| **Data Formats** | Generic blobs | NPZ/HDF5/TFRecord |
| **Metrics** | Storage-centric | Dual (storage + AI/ML) |
| **Deployment** | SSH + Container | SSH + Container |
| **Scale Patterns** | Scale-out + Scale-up | Scale-out + Scale-up |
| **Multi-Cloud** | ✅ | ✅ |
| **HDR Histograms** | ✅ | ✅ |
| **s3dlio Integration** | ✅ v0.9.10 | ✅ v0.9.10 |

## Conclusion

By adopting the proven distributed testing architecture from sai3-bench while maintaining dl-driver's AI/ML focus and DLIO compatibility, we can create a powerful multi-host testing tool that serves both communities. The phased approach ensures we can deliver value incrementally while managing complexity.

The key differentiator will remain: **dl-driver is DLIO-compatible and AI/ML-focused**, while sai3-bench remains generic. Both benefit from shared infrastructure patterns and the powerful s3dlio library.

---

**Next Steps**:
1. Review and approve this enhancement plan
2. Create GitHub issues for each phase
3. Begin Phase 1 implementation
4. Regular progress reviews every 2 weeks

**Document Version**: Draft v1.0  
**Last Updated**: October 22, 2025
