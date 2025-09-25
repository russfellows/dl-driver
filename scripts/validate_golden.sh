#!/bin/bash
# Validate dl-driver output against golden references

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BINARY_PATH="$PROJECT_ROOT/target/release/dl-driver"
GOLDENS_DIR="$PROJECT_ROOT/docs/goldens"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🧪 dl-driver Golden Reference Validator${NC}"
echo "=============================================="

# Function to validate a specific benchmark against golden reference
validate_benchmark() {
    local benchmark_name="$1"
    local golden_file="$2"
    
    echo -e "${BLUE}🔍 Validating $benchmark_name against golden reference${NC}"
    
    if [[ ! -f "$golden_file" ]]; then
        echo -e "${RED}❌ Golden reference not found: $golden_file${NC}"
        return 1
    fi
    
    # Create temporary test data and config (similar to golden generation)
    local temp_dir=$(mktemp -d)
    local test_data_dir="$temp_dir/test_data"
    local test_config="$temp_dir/test_config.yaml"
    local current_output="$temp_dir/current_output.json"
    
    # Generate test data
    mkdir -p "$test_data_dir"
    for ((i=0; i<20; i++)); do
        local filename=$(printf "train_file_%06d.npz" $i)
        printf "PK" > "$test_data_dir/$filename"
        dd if=/dev/zero bs=1022 count=1 >> "$test_data_dir/$filename" 2>/dev/null
    done
    
    # Create test config
    cat > "$test_config" << EOF
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
  enabled: false
EOF
    
    # Run current dl-driver
    if "$BINARY_PATH" mlperf \
        --config "$test_config" \
        --format json \
        --max-epochs 2 \
        --max-steps 50 \
        --output "$current_output"; then
        
        echo -e "${GREEN}✅ Generated current output${NC}"
        
        # Compare with golden reference using jq
        if command -v jq >/dev/null 2>&1; then
            validate_with_jq "$golden_file" "$current_output" "$benchmark_name"
        else
            echo -e "${YELLOW}⚠️  jq not available, skipping detailed validation${NC}"
        fi
    else
        echo -e "${RED}❌ Failed to run benchmark for $benchmark_name${NC}"
        rm -rf "$temp_dir"
        return 1
    fi
    
    # Cleanup
    rm -rf "$temp_dir"
    return 0
}

# Function to perform detailed validation using jq
validate_with_jq() {
    local golden_file="$1"
    local current_file="$2"
    local benchmark_name="$3"
    
    echo -e "${YELLOW}🔍 Comparing metrics for $benchmark_name${NC}"
    
    # Extract key metrics from both files
    local golden_throughput=$(jq -r '.throughput_samples_per_sec' "$golden_file")
    local current_throughput=$(jq -r '.throughput_samples_per_sec' "$current_file")
    
    local golden_files=$(jq -r '.total_files_processed' "$golden_file")
    local current_files=$(jq -r '.total_files_processed' "$current_file")
    
    local golden_samples=$(jq -r '.total_samples_processed' "$golden_file")
    local current_samples=$(jq -r '.total_samples_processed' "$current_file")
    
    # Validate functional metrics (exact match required)
    if [[ "$golden_files" != "$current_files" ]]; then
        echo -e "${RED}❌ Files processed mismatch: $current_files != $golden_files${NC}"
        return 1
    fi
    
    if [[ "$golden_samples" != "$current_samples" ]]; then
        echo -e "${RED}❌ Samples processed mismatch: $current_samples != $golden_samples${NC}"
        return 1
    fi
    
    # Validate performance metrics (with tolerance)
    local throughput_diff=$(echo "scale=2; ($current_throughput - $golden_throughput) / $golden_throughput * 100" | bc -l)
    local throughput_abs_diff=$(echo "scale=2; if ($throughput_diff < 0) -$throughput_diff else $throughput_diff" | bc -l)
    
    if (( $(echo "$throughput_abs_diff > 10.0" | bc -l) )); then
        echo -e "${RED}❌ Throughput variance too high: ${throughput_diff}%${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✅ Functional metrics match${NC}"
    echo -e "${GREEN}✅ Performance within tolerance (${throughput_diff}%)${NC}"
    
    # Show comparison
    echo -e "${BLUE}📊 Comparison for $benchmark_name:${NC}"
    echo "  Throughput: $current_throughput vs $golden_throughput samples/sec (${throughput_diff}%)"
    echo "  Files: $current_files (✓)"
    echo "  Samples: $current_samples (✓)"
    
    return 0
}

# Main execution
if [[ $# -eq 1 ]]; then
    # Validate specific benchmark
    benchmark_name="$1"
    golden_file="$GOLDENS_DIR/${benchmark_name,,}_reference.json"
    
    if validate_benchmark "$benchmark_name" "$golden_file"; then
        echo -e "${GREEN}🎉 $benchmark_name validation: PASSED${NC}"
    else
        echo -e "${RED}❌ $benchmark_name validation: FAILED${NC}"
        exit 1
    fi
else
    # Validate all available golden references
    echo -e "${BLUE}🔍 Validating all available golden references...${NC}"
    
    success_count=0
    total_count=0
    
    for golden_file in "$GOLDENS_DIR"/*_reference.json; do
        if [[ -f "$golden_file" ]]; then
            benchmark_name=$(basename "$golden_file" | sed 's/_reference\.json$//' | tr '[:lower:]' '[:upper:]')
            ((total_count++))
            
            echo ""
            if validate_benchmark "$benchmark_name" "$golden_file"; then
                echo -e "${GREEN}✅ $benchmark_name: PASSED${NC}"
                ((success_count++))
            else
                echo -e "${RED}❌ $benchmark_name: FAILED${NC}"
            fi
        fi
    done
    
    # Summary
    echo ""
    echo "=============================================="
    echo -e "${BLUE}📊 Validation Summary${NC}"
    echo -e "Success rate: ${GREEN}$success_count${NC}/$total_count benchmarks"
    
    if [[ $success_count -eq $total_count ]]; then
        echo -e "${GREEN}🎉 All validations passed!${NC}"
        exit 0
    else
        echo -e "${RED}⚠️  Some validations failed${NC}"
        exit 1
    fi
fi