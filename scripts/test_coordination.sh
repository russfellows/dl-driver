#!/bin/bash

# Simple coordination test script
echo "🧪 Testing coordination system with 2 ranks"
echo "============================================"

cd /home/eval/Documents/Rust-Devel/dl-driver

# Build first
echo "🔨 Building..."
cargo build --release --bin test_coordination

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "✅ Build successful"

# Test coordination in background
echo "🚀 Starting rank 0..."
./target/release/test_coordination 0 2 > /tmp/coord_rank_0.log 2>&1 &
PID0=$!

echo "🚀 Starting rank 1..."  
./target/release/test_coordination 1 2 > /tmp/coord_rank_1.log 2>&1 &
PID1=$!

echo "⏳ Waiting for coordination test (max 30 seconds)..."
echo "   Rank 0 PID: $PID0"
echo "   Rank 1 PID: $PID1"

# Wait for completion or timeout
timeout=30
elapsed=0
while [ $elapsed -lt $timeout ]; do
    if ! kill -0 $PID0 2>/dev/null && ! kill -0 $PID1 2>/dev/null; then
        echo "✅ Both processes completed"
        break
    fi
    sleep 1
    elapsed=$((elapsed + 1))
    if [ $((elapsed % 5)) -eq 0 ]; then
        echo "   Still running... (${elapsed}s elapsed)"
    fi
done

# Check if processes are still running (timeout)
if kill -0 $PID0 2>/dev/null || kill -0 $PID1 2>/dev/null; then
    echo "⚠️  Timeout reached, killing processes"
    kill -TERM $PID0 $PID1 2>/dev/null
    sleep 2
    kill -KILL $PID0 $PID1 2>/dev/null
    echo "❌ Coordination test timed out"
else
    echo "🎉 Coordination test completed"
fi

echo
echo "📋 Rank 0 log:"
echo "=============="
cat /tmp/coord_rank_0.log
echo
echo "📋 Rank 1 log:"
echo "=============="
cat /tmp/coord_rank_1.log
echo
echo "🧹 Cleanup complete"