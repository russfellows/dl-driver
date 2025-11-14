#!/bin/bash
# Test script for distributed multi-rank file sharding (v0.8.8 Priority 0, Phase 1)
# Tests that agents get unique, non-overlapping file subsets

set -e

echo "=========================================="
echo "Distributed Multi-Rank Sharding Test"
echo "v0.8.8 Priority 0, Phase 1, Step 3"
echo "=========================================="
echo

# Configuration
TEST_DIR="/tmp/dl-driver-rank-test"
CONFIG_FILE="tests/test_distributed_rank_sharding.yaml"
AGENT1_PORT=50051
AGENT2_PORT=50052
NUM_FILES=20

# Cleanup function
cleanup() {
    echo
    echo "Cleaning up..."
    pkill -f "dl-driver.*agent.*${AGENT1_PORT}" || true
    pkill -f "dl-driver.*agent.*${AGENT2_PORT}" || true
    rm -rf "${TEST_DIR}"
    echo "Cleanup complete"
}

trap cleanup EXIT

# Step 1: Clean and prepare test directory
echo "Step 1: Preparing test data directory..."
rm -rf "${TEST_DIR}"
mkdir -p "${TEST_DIR}"

# For distributed mode with file://, controller creates agent-specific subdirectories
# But for sharding test, we want all agents to see the SAME files
# So we'll create symlinks from agent-N dirs to the shared data
mkdir -p "${TEST_DIR}/agent-0"
mkdir -p "${TEST_DIR}/agent-1"

# Generate test files in a shared location
SHARED_DIR="${TEST_DIR}/shared"
mkdir -p "${SHARED_DIR}"
echo "Generating ${NUM_FILES} test files in shared directory..."
for i in $(seq 0 $((NUM_FILES - 1))); do
    filename=$(printf "file_%06d.npz" $i)
    dd if=/dev/urandom of="${SHARED_DIR}/${filename}" bs=1024 count=10 2>/dev/null
done

# Create symlinks so agents can access the same files
echo "Creating symlinks for agent access..."
for i in $(seq 0 $((NUM_FILES - 1))); do
    filename=$(printf "file_%06d.npz" $i)
    ln -s "${SHARED_DIR}/${filename}" "${TEST_DIR}/agent-0/${filename}"
    ln -s "${SHARED_DIR}/${filename}" "${TEST_DIR}/agent-1/${filename}"
done

echo "✓ Generated ${NUM_FILES} files accessible to all agents"
ls -lh "${SHARED_DIR}" | head -5
echo "..."
echo

# Step 2: Start agents
echo "Step 2: Starting 2 agents..."
echo "Agent 1 on port ${AGENT1_PORT}..."
cargo run --bin dl_driver_agent --release -- --port ${AGENT1_PORT} > /tmp/agent1.log 2>&1 &
AGENT1_PID=$!

echo "Agent 2 on port ${AGENT2_PORT}..."
cargo run --bin dl_driver_agent --release -- --port ${AGENT2_PORT} > /tmp/agent2.log 2>&1 &
AGENT2_PID=$!

echo "Waiting for agents to start..."
sleep 3

# Verify agents are running
if ! kill -0 ${AGENT1_PID} 2>/dev/null; then
    echo "ERROR: Agent 1 failed to start"
    cat /tmp/agent1.log
    exit 1
fi

if ! kill -0 ${AGENT2_PID} 2>/dev/null; then
    echo "ERROR: Agent 2 failed to start"
    cat /tmp/agent2.log
    exit 1
fi

echo "✓ Both agents started successfully"
echo "   Agent 1 (PID ${AGENT1_PID}): localhost:${AGENT1_PORT}"
echo "   Agent 2 (PID ${AGENT2_PID}): localhost:${AGENT2_PORT}"
echo

# Step 3: Run distributed workload
echo "Step 3: Running distributed workload with 2 agents..."
echo "Expected behavior:"
echo "   - Agent 0 (rank 0) should process files: 0, 2, 4, 6, 8, 10, 12, 14, 16, 18 (interleaved)"
echo "   - Agent 1 (rank 1) should process files: 1, 3, 5, 7, 9, 11, 13, 15, 17, 19 (interleaved)"
echo "   - Total files per agent: 10 each"
echo "   - No file overlap"
echo

cargo run --bin dl-driver --release -- distributed run \
    --config "${CONFIG_FILE}" \
    --agents "localhost:${AGENT1_PORT},localhost:${AGENT2_PORT}"

echo
echo "=========================================="
echo "Test Results"
echo "=========================================="

# Step 4: Analyze results
echo
echo "Step 4: Analyzing agent logs for file processing..."
echo

echo "Agent 1 log (rank 0):"
grep -i "rank.*files" /tmp/agent1.log || echo "(no rank info found)"
grep -i "discovered.*files" /tmp/agent1.log || echo "(no file discovery info)"
echo

echo "Agent 2 log (rank 1):"
grep -i "rank.*files" /tmp/agent2.log || echo "(no rank info found)"
grep -i "discovered.*files" /tmp/agent2.log || echo "(no file discovery info)"
echo

# Check for sharding messages
echo "Checking for sharding strategy messages..."
grep -i "shard" /tmp/agent1.log | head -3 || echo "(no sharding messages in agent 1)"
grep -i "shard" /tmp/agent2.log | head -3 || echo "(no sharding messages in agent 2)"
echo

echo "=========================================="
echo "Test Complete!"
echo "=========================================="
echo
echo "Check results in: /tmp/dl-driver-distributed-test-results"
echo "Agent logs: /tmp/agent1.log, /tmp/agent2.log"
echo
echo "Expected outcome:"
echo "✓ Each agent should report processing 10 files (20 total / 2 agents)"
echo "✓ Each agent should log its rank and sharding strategy"
echo "✓ No errors in distributed execution"
