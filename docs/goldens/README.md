# Golden Reference Reports

This directory contains reference MLPerf reports and validation specifications for dl-driver's compatibility testing with DLIO benchmarks.

## Purpose

Golden reference reports serve as baselines for validating that dl-driver produces consistent, comparable results to the original DLIO benchmark suite. These references enable:

1. **Regression Testing**: Ensure dl-driver updates don't break MLPerf compatibility
2. **Performance Validation**: Verify performance metrics are within acceptable ranges
3. **Format Verification**: Confirm output format consistency with DLIO standards
4. **Deterministic Validation**: Ensure reproducible results across runs

## Structure

### Reference Reports
- `unet3d_reference.json` - UNet3D benchmark baseline results
- `bert_reference.json` - BERT benchmark baseline results  
- `resnet_reference.json` - ResNet benchmark baseline results
- `cosmoflow_reference.json` - CosmoFlow benchmark baseline results

### Tolerance Specifications
- `tolerance.json` - Acceptable variance thresholds for metrics validation
- `validation_schema.json` - JSON schema for report structure validation

### Test Configurations
- `test_configs/` - DLIO configs used to generate golden references
- `validation_scripts/` - Automated validation test scripts

## Usage

### Generate New Golden References
```bash
# Generate reference report for UNet3D
./target/release/dl-driver run --mlperf --config docs/goldens/test_configs/unet3d.yaml --format json --output docs/goldens/unet3d_reference.json

# Generate for all benchmarks
scripts/generate_golden_references.sh
```

### Validate Against Golden References
```bash
# Run validation test
cargo test --test golden_validation

# Validate specific benchmark
scripts/validate_golden.sh unet3d
```

## Tolerance Guidelines

### Performance Metrics
- **Throughput**: ±5% variance acceptable
- **Latency P50/P95/P99**: ±10% variance acceptable
- **Memory usage**: ±15% variance acceptable

### Functional Metrics
- **Files processed**: Exact match required
- **Bytes read/written**: Exact match required
- **Batch count**: Exact match required
- **Access order**: Exact sequence match (for deterministic validation)

### Version Compatibility
- **Major version changes**: May require new golden references
- **Minor version changes**: Should maintain compatibility within tolerance
- **Patch versions**: Must maintain exact compatibility

## Maintenance

Golden references should be updated when:
1. **Intentional performance improvements** that exceed tolerance thresholds
2. **New MLPerf specification compliance** requirements
3. **DLIO benchmark suite updates** that change expected behavior
4. **s3dlio library updates** that affect performance characteristics

Always document the reason for golden reference updates in the commit message and update this README accordingly.