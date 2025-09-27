#!/bin/bash
set -e

echo "🚀 Testing Complete Shared Memory Multi-Rank Coordination (NO TEMP FILES)"
echo "================================================================="

# Cleanup any previous coordination state
sudo rm -f /dev/shm/dl_driver_coord_*
pkill -f "dl-driver run" || true
sleep 2

cd /home/eval/Documents/Rust-Devel/dl-driver

# Start rank 0 in background
echo "🏁 Starting Rank 0..."
RUST_LOG=debug ./target/release/dl-driver -vv run -c tests/dlio_configs/minimal_config.yaml --world-size 2 --rank 0 &
RANK0_PID=$!

# Wait a bit for rank 0 to initialize
sleep 3

# Start rank 1
echo "🏁 Starting Rank 1..."
RUST_LOG=debug ./target/release/dl-driver -vv run -c tests/dlio_configs/minimal_config.yaml --world-size 2 --rank 1 &
RANK1_PID=$!

echo "⏳ Waiting for coordination to complete..."

# Wait for both processes
wait $RANK0_PID
RANK0_EXIT=$?

wait $RANK1_PID  
RANK1_EXIT=$?

echo ""
echo "📊 Results:"
echo "  Rank 0 exit code: $RANK0_EXIT"
echo "  Rank 1 exit code: $RANK1_EXIT"

# Check for shared memory cleanup
echo ""
echo "🧹 Shared memory cleanup check:"
ls /dev/shm/dl_driver_coord_* 2>/dev/null || echo "✅ All shared memory segments cleaned up"

if [ $RANK0_EXIT -eq 0 ] && [ $RANK1_EXIT -eq 0 ]; then
    echo ""
    echo "🎉 SUCCESS: Multi-rank coordination with shared memory (NO TEMP FILES) completed!"
else
    echo ""
    echo "❌ FAILED: One or more ranks failed"
    exit 1
fi