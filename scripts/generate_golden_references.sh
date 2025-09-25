#!/bin/bash
# Generate golden reference reports for dl-driver MLPerf compatibility testing

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BINARY_PATH="$PROJECT_ROOT/target/release/dl-driver"
GOLDENS_DIR="$PROJECT_ROOT/docs/goldens"
CONFIGS_DIR="$GOLDENS_DIR/test_configs"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 dl-driver Golden Reference Generator${NC}"
echo "================================================="

# Check if binary exists
if [[ ! -f "$BINARY_PATH" ]]; then
    echo -e "${RED}❌ dl-driver binary not found at: $BINARY_PATH${NC}"
    echo -e "${YELLOW}💡 Run 'cargo build --release' first${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Found dl-driver binary${NC}"

# Check if config directory exists
if [[ ! -d "$CONFIGS_DIR" ]]; then
    echo -e "${RED}❌ Test configs directory not found: $CONFIGS_DIR${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Found test configs directory${NC}"

# Create test data directories
TEST_DATA_ROOT="/mnt/vast1/dl_driver_golden_test_data"
echo -e "${BLUE}📁 Creating test data directory: $TEST_DATA_ROOT${NC}"
mkdir -p "$TEST_DATA_ROOT"

# Function to generate test data for a specific format
generate_test_data() {
    local format="$1"
    local num_files="$2"
    local data_dir="$3"
    
    echo -e "${YELLOW}🔧 Generating $num_files $format test files in $data_dir${NC}"
    mkdir -p "$data_dir"
    
    case "$format" in
        "npz")
            for ((i=0; i<num_files; i++)); do
                local filename=$(printf "train_file_%06d.npz" $i)
                # Create minimal NPZ-like file (placeholder for now)
                printf "PK" > "$data_dir/$filename"
                dd if=/dev/zero bs=1022 count=1 >> "$data_dir/$filename" 2>/dev/null
            done
            ;;
        "hdf5"|"h5")
            for ((i=0; i<num_files; i++)); do
                local filename=$(printf "train_file_%06d.h5" $i)
                # Create minimal HDF5-like file (placeholder for now)
                dd if=/dev/zero bs=1024 count=1 of="$data_dir/$filename" 2>/dev/null
            done
            ;;
        *)
            echo -e "${RED}❌ Unsupported format: $format${NC}"
            return 1
            ;;
    esac
    
    echo -e "${GREEN}✅ Generated $num_files $format files${NC}"
}

# Function to create modified config for testing
create_test_config() {
    local benchmark_name="$1"
    local original_config="$2"
    local test_data_dir="$3"
    local output_config="$4"
    
    echo -e "${YELLOW}🔧 Creating test config for $benchmark_name${NC}"
    
    # Create a simplified config for golden reference generation
    cat > "$output_config" << EOF
model:
  name: "$benchmark_name"
  
dataset:
  data_folder: "file://$test_data_dir"
  format: "npz"
  num_files_train: 20
  record_length: 1024
  
reader:
  batch_size: 8
  read_threads: 4
  prefetch: 8
  shuffle: true
  seed: 42
  
train:
  epochs: 2
  steps: 50

checkpoint:
  enabled: true
  steps_between_checkpoints: 20
  compression: zstd
  folder: "file://$test_data_dir/checkpoints"

profiling:
  enabled: true
EOF

    echo -e "${GREEN}✅ Created test config: $output_config${NC}"
}

# Function to generate golden reference for a benchmark
generate_golden_reference() {
    local benchmark_name="$1"
    local original_config="$2"
    
    echo ""
    echo -e "${BLUE}📊 Generating golden reference for $benchmark_name${NC}"
    echo "----------------------------------------"
    
    # Create test data directory
    local test_data_dir="$TEST_DATA_ROOT/${benchmark_name,,}"
    local test_config="$test_data_dir/test_config.yaml"
    local golden_output="$GOLDENS_DIR/${benchmark_name,,}_reference.json"
    
    # Generate test data
    generate_test_data "npz" 20 "$test_data_dir"
    
    # Create test configuration
    create_test_config "$benchmark_name" "$original_config" "$test_data_dir" "$test_config"
    
    # Run dl-driver to generate golden reference
    echo -e "${YELLOW}🏃 Running dl-driver MLPerf benchmark for $benchmark_name...${NC}"
    
    if "$BINARY_PATH" mlperf \
        --config "$test_config" \
        --format json \
        --max-epochs 2 \
        --max-steps 50 \
        --output "$golden_output"; then
        
        echo -e "${GREEN}✅ Generated golden reference: $golden_output${NC}"
        
        # Pretty print key metrics from the golden reference
        if command -v jq >/dev/null 2>&1; then
            echo -e "${BLUE}📋 Key metrics for $benchmark_name:${NC}"
            jq -r '
                "  • Throughput: \(.throughput_samples_per_sec) samples/sec",
                "  • Files processed: \(.total_files_processed)",
                "  • Total time: \(.total_execution_time_secs)s",
                "  • I/O P95 latency: \(.io_p95_latency_ms)ms"
            ' "$golden_output"
        fi
    else
        echo -e "${RED}❌ Failed to generate golden reference for $benchmark_name${NC}"
        return 1
    fi
}

# Main execution
echo ""
echo -e "${BLUE}🎯 Starting golden reference generation...${NC}"

# List of benchmarks to generate
BENCHMARKS=(
    "UNet3D:unet3d_config.yaml"
    "BERT:bert_config.yaml" 
    "ResNet:resnet_config.yaml"
    "CosmoFlow:cosmoflow_config.yaml"
)

success_count=0
total_count=${#BENCHMARKS[@]}

for benchmark_entry in "${BENCHMARKS[@]}"; do
    IFS=':' read -r benchmark_name config_file <<< "$benchmark_entry"
    original_config="$CONFIGS_DIR/$config_file"
    
    if [[ -f "$original_config" ]]; then
        if generate_golden_reference "$benchmark_name" "$original_config"; then
            ((success_count++))
        fi
    else
        echo -e "${RED}❌ Config file not found: $original_config${NC}"
    fi
done

# Summary
echo ""
echo "================================================="
echo -e "${BLUE}📊 Golden Reference Generation Complete${NC}"
echo -e "Success rate: ${GREEN}$success_count${NC}/$total_count benchmarks"

if [[ $success_count -eq $total_count ]]; then
    echo -e "${GREEN}🎉 All golden references generated successfully!${NC}"
    echo ""
    echo -e "${BLUE}📁 Generated files:${NC}"
    find "$GOLDENS_DIR" -name "*_reference.json" -exec basename {} \; | sort
    echo ""
    echo -e "${YELLOW}💡 Next steps:${NC}"
    echo "  1. Review generated golden references for accuracy"
    echo "  2. Run validation tests: cargo test --test dlio_mlperf_compatibility_tests"
    echo "  3. Commit golden references to version control"
    exit 0
else
    echo -e "${RED}⚠️  Some golden references failed to generate${NC}"
    exit 1
fi