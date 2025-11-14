#!/bin/bash
# Phase 2 Multi-Rank Testing
# Tests distributed execution with ranks_per_agent > 1
# 
# This script ONLY runs dl-driver commands - all file generation/reading
# is handled by dl-driver itself through YAML configs.

set -u
set -o pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

PROJECT_ROOT="/home/eval/Documents/Code/dl-driver"
cd "$PROJECT_ROOT"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Phase 2 Multi-Rank Testing${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Kill any existing agents
pkill -f dl_driver_agent 2>/dev/null || true
sleep 1

# Clean previous test data
echo -e "${YELLOW}Cleaning previous test data...${NC}"
rm -rf /tmp/dl-driver-phase2-test
echo ""

# Step 1: Generate test data using dl-driver
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}STEP 1: Generate Test Data${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Running: dl-driver run --config tests/phase2_generate_data.yaml${NC}"
./target/release/dl-driver run --config tests/phase2_generate_data.yaml

GEN_EXIT=$?
if [ $GEN_EXIT -ne 0 ]; then
    echo -e "${RED}✗ Data generation FAILED (exit code: $GEN_EXIT)${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Data generation complete${NC}"
echo ""

# Verify files were created
FILE_COUNT=$(ls -1 /tmp/dl-driver-phase2-test/*.npz 2>/dev/null | wc -l)
echo -e "${YELLOW}Files created: $FILE_COUNT${NC}"
if [ $FILE_COUNT -eq 0 ]; then
    echo -e "${RED}✗ No NPZ files found!${NC}"
    exit 1
fi
echo ""

sleep 2

# Step 2: Test Phase 1 baseline (1 rank per agent)
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}TEST 1: Phase 1 Baseline (1 rank/agent)${NC}"
echo -e "${BLUE}2 agents × 1 rank = 2 total ranks${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Starting agents on ports 50051, 50052...${NC}"
./target/release/dl_driver_agent --port 50051 > /tmp/agent_50051.log 2>&1 &
AGENT1_PID=$!
./target/release/dl_driver_agent --port 50052 > /tmp/agent_50052.log 2>&1 &
AGENT2_PID=$!
sleep 2
echo ""

echo -e "${YELLOW}Running: 2 agents × 1 rank (interleaved sharding)${NC}"
timeout 60s ./target/release/dl-driver distributed run \
    --config tests/phase2_distributed_read.yaml \
    --agents "localhost:50051,localhost:50052" \
    --shard-strategy interleaved \
    --ranks-per-agent 1 \
    --shared-storage

TEST1_EXIT=$?

echo ""
echo -e "${YELLOW}Stopping agents...${NC}"
kill $AGENT1_PID $AGENT2_PID 2>/dev/null || true
wait 2>/dev/null || true
sleep 1
echo ""

if [ $TEST1_EXIT -eq 124 ]; then
    echo -e "${RED}✗ TEST 1 TIMED OUT${NC}"
    exit 1
elif [ $TEST1_EXIT -ne 0 ]; then
    echo -e "${RED}✗ TEST 1 FAILED (exit code: $TEST1_EXIT)${NC}"
    exit 1
else
    echo -e "${GREEN}✓ TEST 1 PASSED${NC}"
fi

echo ""
sleep 3

# Step 3: Test Phase 2 (2 ranks per agent)
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}TEST 2: Phase 2 Multi-Rank (2 ranks/agent)${NC}"
echo -e "${BLUE}2 agents × 2 ranks = 4 total ranks${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo -e "${YELLOW}Starting agents on ports 50051, 50052...${NC}"
./target/release/dl_driver_agent --port 50051 > /tmp/agent_50051.log 2>&1 &
AGENT1_PID=$!
./target/release/dl_driver_agent --port 50052 > /tmp/agent_50052.log 2>&1 &
AGENT2_PID=$!
sleep 2
echo ""

echo -e "${YELLOW}Running: 2 agents × 2 ranks (interleaved sharding)${NC}"
timeout 60s ./target/release/dl-driver distributed run \
    --config tests/phase2_distributed_read.yaml \
    --agents "localhost:50051,localhost:50052" \
    --shard-strategy interleaved \
    --ranks-per-agent 2 \
    --shared-storage

TEST2_EXIT=$?

echo ""
echo -e "${YELLOW}Stopping agents...${NC}"
kill $AGENT1_PID $AGENT2_PID 2>/dev/null || true
wait 2>/dev/null || true
sleep 1
echo ""

if [ $TEST2_EXIT -eq 124 ]; then
    echo -e "${RED}✗ TEST 2 TIMED OUT${NC}"
    exit 1
elif [ $TEST2_EXIT -ne 0 ]; then
    echo -e "${RED}✗ TEST 2 FAILED (exit code: $TEST2_EXIT)${NC}"
    exit 1
else
    echo -e "${GREEN}✓ TEST 2 PASSED${NC}"
fi

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}All tests completed successfully!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "${YELLOW}Agent logs:${NC}"
echo "  /tmp/agent_50051.log"
echo "  /tmp/agent_50052.log"
echo ""
