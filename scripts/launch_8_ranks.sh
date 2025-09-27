#!/bin/bash
# Multi-rank dl-driver launcher script
# Simulates 8-GPU distributed data loading on single host

set -e

# Configuration
WORLD_SIZE=8
CONFIG_FILE="${1:-tests/dlio_configs/large_scale_threading_test.yaml}"
RESULTS_DIR="/mnt/vast1/dl_driver_multi_rank_results"
DATA_DIR="/mnt/vast1/dl_driver_large_threading_test"
DL_DRIVER_BIN="./target/release/dl-driver"

# Validate inputs
if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "Error: Config file not found: $CONFIG_FILE"
    echo "Usage: $0 [config_file]"
    exit 1
fi

if [[ ! -f "$DL_DRIVER_BIN" ]]; then
    echo "Error: dl-driver binary not found: $DL_DRIVER_BIN"
    echo "Run: cargo build --release"
    exit 1
fi

# Create results directory
mkdir -p "$RESULTS_DIR"

# Calculate synchronized start time (5 seconds from now)
START_AT=$(python3 -c "import time; print(int(time.time()) + 5)")

echo "🚀 Multi-Rank DLIO Launcher"
echo "=========================="
echo "Config: $CONFIG_FILE"
echo "World size: $WORLD_SIZE"
echo "Results dir: $RESULTS_DIR"
echo "Start time: $START_AT ($(date -d @$START_AT))"
echo ""

# Generate file list for sharding
echo "📁 Generating file list for sharding..."
if [[ -d "$DATA_DIR" ]]; then
    find "$DATA_DIR" -name "*.npz" | sort > "$RESULTS_DIR/all_files.txt"
    TOTAL_FILES=$(wc -l < "$RESULTS_DIR/all_files.txt")
    echo "Found $TOTAL_FILES files for sharding"
else
    echo "Warning: Data directory not found: $DATA_DIR"
    echo "Files will be discovered during execution"
fi

# Launch ranks with CPU pinning
echo ""
echo "🔥 Launching $WORLD_SIZE ranks..."
PIDS=()

for rank in $(seq 0 $((WORLD_SIZE-1))); do
    # CPU affinity: 8 cores per rank (adjust based on your system)
    # Assuming 64+ cores available, distribute evenly
    CORES_PER_RANK=8
    START_CORE=$((rank * CORES_PER_RANK))
    END_CORE=$((START_CORE + CORES_PER_RANK - 1))
    
    # NUMA node (assuming 2 NUMA nodes, adjust as needed)
    NUMA_NODE=$((rank / 4))  # ranks 0-3 on NUMA0, 4-7 on NUMA1
    
    LOG_FILE="$RESULTS_DIR/rank_${rank}.log"
    RESULTS_FILE="$RESULTS_DIR/rank_${rank}.json"
    
    echo "Rank $rank: cores $START_CORE-$END_CORE, NUMA node $NUMA_NODE"
    
    # Launch with CPU and NUMA pinning
    taskset -c "$START_CORE-$END_CORE" \
    numactl --cpunodebind="$NUMA_NODE" --membind="$NUMA_NODE" \
    "$DL_DRIVER_BIN" run \
        --config "$CONFIG_FILE" \
        --rank "$rank" \
        --world-size "$WORLD_SIZE" \
        --start-at-epoch "$START_AT" \
        --shard-strategy "interleaved" \
        --results "$RESULTS_FILE" \
        --filelist "$RESULTS_DIR/all_files.txt" \
        > "$LOG_FILE" 2>&1 &
    
    PIDS+=($!)
done

echo ""
echo "⏳ Waiting for synchronized start at $(date -d @$START_AT)..."
sleep $(( START_AT - $(date +%s) + 1 ))

echo "🏃 All ranks started! Waiting for completion..."
echo "Monitor progress with: tail -f $RESULTS_DIR/rank_*.log"
echo ""

# Wait for all processes to complete
FAILED_RANKS=()
for i in "${!PIDS[@]}"; do
    pid=${PIDS[$i]}
    if wait "$pid"; then
        echo "✅ Rank $i (PID $pid) completed successfully"
    else
        echo "❌ Rank $i (PID $pid) failed"
        FAILED_RANKS+=($i)
    fi
done

echo ""
if [[ ${#FAILED_RANKS[@]} -eq 0 ]]; then
    echo "🎉 All ranks completed successfully!"
    
    # Aggregate results
    echo ""
    echo "📊 Aggregating results..."
    "$DL_DRIVER_BIN" aggregate \
        --inputs "$RESULTS_DIR/rank_*.json" \
        --output "$RESULTS_DIR/aggregated_results.json" \
        --strict-au
    
    echo ""
    echo "📈 Results Summary:"
    echo "=================="
    echo "Individual rank logs: $RESULTS_DIR/rank_*.log"
    echo "Individual rank results: $RESULTS_DIR/rank_*.json"  
    echo "Aggregated results: $RESULTS_DIR/aggregated_results.json"
    echo ""
    echo "View aggregated results with:"
    echo "cat $RESULTS_DIR/aggregated_results.json | jq '.aggregated_results.global_metrics'"
    
else
    echo "💥 ${#FAILED_RANKS[@]} ranks failed: ${FAILED_RANKS[*]}"
    echo "Check logs in $RESULTS_DIR/rank_*.log for details"
    exit 1
fi