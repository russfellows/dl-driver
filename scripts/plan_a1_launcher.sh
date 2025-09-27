#!/bin/bash
# Plan A1: True Multi-GPU DLIO Launcher for Same-Host GPU Scaling
# Automatically detects GPUs and launches one rank per GPU with proper affinity

set -e

# Configuration
SCRIPT_NAME="plan_a1_launcher.sh"
DEFAULT_CONFIG="tests/dlio_configs/multi_rank_test.yaml"
DEFAULT_DATA_FOLDER="file:///mnt/vast1/plan_a1_multi_gpu"

# Parse command line arguments
CONFIG="${1:-$DEFAULT_CONFIG}"
DATA_FOLDER="${2:-$DEFAULT_DATA_FOLDER}"
RESULTS_DIR="${3:-/tmp/plan_a1_results}"
SIMULATION_MODE="${4:-true}"  # Default to simulation mode
GPU_COUNT="${5:-}"  # Optional: override GPU count for simulation

echo "🎯 Plan A1: Multi-GPU DLIO Launcher"
echo "================================="
echo "Usage: $0 [CONFIG] [DATA_FOLDER] [RESULTS_DIR] [SIMULATION_MODE] [GPU_COUNT]"
echo "  CONFIG: DLIO config file (default: $DEFAULT_CONFIG)"
echo "  DATA_FOLDER: Storage URI (default: $DEFAULT_DATA_FOLDER)" 
echo "  RESULTS_DIR: Output directory (default: /tmp/plan_a1_results)"
echo "  SIMULATION_MODE: true for pure simulation, false for [FUTURE] GPU detection mode (default: true)"
echo "  GPU_COUNT: Number of GPUs to simulate (default: 4, ignored in GPU detection mode)"
echo "  NOTE: Both modes use CPU simulation - GPU detection is for future GPU integration"
echo
echo "Config: $CONFIG"
echo "Data folder: $DATA_FOLDER"
echo "Results dir: $RESULTS_DIR"
echo "Simulation mode: $SIMULATION_MODE"
echo

# Determine GPU configuration
if [ "$SIMULATION_MODE" = "true" ]; then
    # Simulation mode: use specified count or default to 4 GPUs
    if [ -n "$GPU_COUNT" ]; then
        echo "🎮 SIMULATION MODE: Using $GPU_COUNT simulated GPUs (override)"
    else
        GPU_COUNT=4
        echo "🎮 SIMULATION MODE: Using $GPU_COUNT simulated GPUs (default)"
    fi
    GPU_IDS=($(seq 0 $((GPU_COUNT-1))))
    USE_REAL_GPUS_FLAG=""
else
    # GPU detection mode: detect actual hardware for future integration
    echo "🔥 [FUTURE] GPU DETECTION MODE: Detecting hardware for future integration..."
    if command -v nvidia-smi >/dev/null 2>&1; then
        GPU_COUNT=$(nvidia-smi --list-gpus | wc -l)
        echo "🔍 Detected $GPU_COUNT NVIDIA GPUs"
        GPU_IDS=($(nvidia-smi --list-gpus | grep -o 'GPU [0-9]*' | awk '{print $2}'))
    elif command -v rocm-smi >/dev/null 2>&1; then
        GPU_COUNT=$(rocm-smi --showid | grep -c "GPU")
        echo "🔍 Detected $GPU_COUNT AMD GPUs (ROCm)"
        GPU_IDS=($(seq 0 $((GPU_COUNT-1))))
    else
        echo "❌ [FUTURE] GPU detection mode requested but no GPUs detected!"
        echo "💡 Try pure simulation mode: $0 \"$CONFIG\" \"$DATA_FOLDER\" \"$RESULTS_DIR\" true"
        exit 1
    fi
    USE_REAL_GPUS_FLAG="--use-real-gpus"
fi

# Validate GPU count
if [ $GPU_COUNT -lt 1 ]; then
    echo "❌ No GPUs detected! Plan A1 requires at least 1 GPU."
    exit 1
fi

MODE_DESC=$(if [ "$SIMULATION_MODE" = "true" ]; then echo "PURE SIMULATION"; else echo "GPU DETECTION [FUTURE]"; fi)
echo "🚀 Launching $GPU_COUNT ranks (1 per GPU) for realistic $MODE_DESC multi-GPU scaling"
echo

# Create results directory
mkdir -p "$RESULTS_DIR"

# Launch ranks in parallel
PIDS=()
START_TIME=$(date +%s)

for ((RANK=0; RANK<GPU_COUNT; RANK++)); do
    GPU_ID=${GPU_IDS[$RANK]}
    RESULT_FILE="$RESULTS_DIR/rank_${RANK}.json"
    LOG_FILE="$RESULTS_DIR/rank_${RANK}.log"
    
    echo "📱 Launching Rank $RANK on GPU $GPU_ID -> $RESULT_FILE"
    
    # Set CPU affinity to distribute ranks across NUMA domains
    NUMA_NODE=$((RANK % 2))  # Assume 2 NUMA nodes, distribute evenly
    CPU_CORES_PER_RANK=4
    CPU_START=$((RANK * CPU_CORES_PER_RANK))
    CPU_END=$((CPU_START + CPU_CORES_PER_RANK - 1))
    
    # Launch rank with proper CPU/GPU affinity
    taskset -c "$CPU_START-$CPU_END" \
        ./target/release/dl-driver run \
        --config "$CONFIG" \
        --rank $RANK \
        --world-size $GPU_COUNT \
        --gpus $GPU_COUNT \
        $USE_REAL_GPUS_FLAG \
        --start-at-epoch $((START_TIME + 5)) \
        --results "$RESULT_FILE" \
        --shard-strategy interleaved \
        > "$LOG_FILE" 2>&1 &
    
    PIDS+=($!)
    echo "   PID: ${PIDS[$RANK]}, CPU cores: $CPU_START-$CPU_END, NUMA: $NUMA_NODE"
done

echo
echo "⏳ Waiting for all $GPU_COUNT ranks to complete..."
echo "   Monitor progress: tail -f $RESULTS_DIR/rank_*.log"

# Wait for all ranks to complete
SUCCESS_COUNT=0
for ((i=0; i<GPU_COUNT; i++)); do
    echo -n "   Rank $i (PID ${PIDS[$i]}): "
    if wait ${PIDS[$i]}; then
        echo "✅ SUCCESS"
        ((SUCCESS_COUNT++))
    else
        echo "❌ FAILED"
    fi
done

echo
echo "📊 Plan A1 Results Summary:"
echo "========================="
echo "✅ Successful ranks: $SUCCESS_COUNT / $GPU_COUNT"

if [ $SUCCESS_COUNT -eq $GPU_COUNT ]; then
    echo "🎉 All ranks completed successfully!"
    
    # Aggregate results
    echo "🔄 Aggregating multi-GPU results..."
    ./target/release/dl-driver aggregate \
        --inputs "$RESULTS_DIR/rank_*.json" \
        --output "$RESULTS_DIR/plan_a1_global.json"
    
    echo "📈 Global Plan A1 Multi-GPU Results:"
    cat "$RESULTS_DIR/plan_a1_global.json" | jq '.aggregated_results.global_metrics'
    
    echo
    echo "✅ Plan A1 Multi-GPU execution completed successfully!"
    echo "📁 Results: $RESULTS_DIR/plan_a1_global.json"
    echo "📝 Individual logs: $RESULTS_DIR/rank_*.log"
else
    echo "❌ Some ranks failed. Check individual logs in $RESULTS_DIR/"
    exit 1
fi