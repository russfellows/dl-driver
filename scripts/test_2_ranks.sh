#!/bin/bash
# Simple 2-rank launcher for testing multi-rank functionality

set -e

WORLD_SIZE=2
CONFIG_FILE="${1:-tests/dlio_configs/multi_rank_test.yaml}"
RESULTS_DIR="/tmp/dl_driver_2rank_test"
DL_DRIVER_BIN="./target/release/dl-driver"

# Validate inputs
if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "Error: Config file not found: $CONFIG_FILE"
    exit 1
fi

if [[ ! -f "$DL_DRIVER_BIN" ]]; then
    echo "Error: Build dl-driver first: cargo build --release"
    exit 1
fi

# Create results directory
mkdir -p "$RESULTS_DIR"
rm -f "$RESULTS_DIR"/*  # Clean previous results

# Calculate start time
START_AT=$(python3 -c "import time; print(int(time.time()) + 3)")

echo "🧪 Simple 2-Rank Test"
echo "===================="
echo "Config: $CONFIG_FILE"
echo "Results: $RESULTS_DIR"
echo ""

# Launch 2 ranks without CPU pinning for simplicity
echo "🚀 Launching 2 ranks..."

"$DL_DRIVER_BIN" run \
    --config "$CONFIG_FILE" \
    --rank 0 \
    --world-size "$WORLD_SIZE" \
    --start-at-epoch "$START_AT" \
    --shard-strategy "interleaved" \
    --results "$RESULTS_DIR/rank_0.json" \
    > "$RESULTS_DIR/rank_0.log" 2>&1 &
RANK0_PID=$!

"$DL_DRIVER_BIN" run \
    --config "$CONFIG_FILE" \
    --rank 1 \
    --world-size "$WORLD_SIZE" \
    --start-at-epoch "$START_AT" \
    --shard-strategy "interleaved" \
    --results "$RESULTS_DIR/rank_1.json" \
    > "$RESULTS_DIR/rank_1.log" 2>&1 &
RANK1_PID=$!

echo "Waiting for completion..."

if wait "$RANK0_PID" && wait "$RANK1_PID"; then
    echo "✅ Both ranks completed!"
    
    # Show basic results
    echo ""
    echo "📊 Results:"
    echo "Rank 0 log: $RESULTS_DIR/rank_0.log"
    echo "Rank 1 log: $RESULTS_DIR/rank_1.log"
    
    if [[ -f "$RESULTS_DIR/rank_0.json" && -f "$RESULTS_DIR/rank_1.json" ]]; then
        echo ""
        echo "Testing aggregation..."
        "$DL_DRIVER_BIN" aggregate \
            --inputs "$RESULTS_DIR/rank_*.json" \
            --output "$RESULTS_DIR/aggregated.json"
        echo "Aggregated results: $RESULTS_DIR/aggregated.json"
    fi
else
    echo "❌ One or more ranks failed"
    exit 1
fi