#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

#
# test_live_stats_2agent.sh - Test distributed live stats with startup handshake
#
# Tests v0.8.7 features:
#   - Startup handshake (READY/ERROR status)
#   - Live stats streaming every 1s
#   - Microsecond precision displays
#   - Coordinated start timing
#   - Final stats preservation
#
# Uses file:// backend with /mnt/test mount point
#

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
AGENT_COUNT=2
BASE_PORT=50051
TEST_DATA_DIR="/mnt/test/dl-driver-dist-test"
CONFIG_FILE="${PROJECT_ROOT}/tests/dlio_configs/test_distributed_2agent.yaml"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    
    # Kill agent processes
    for i in $(seq 0 $((AGENT_COUNT - 1))); do
        local port=$((BASE_PORT + i))
        local pid=$(lsof -ti:${port} 2>/dev/null || true)
        if [ -n "$pid" ]; then
            log_info "Killing agent on port ${port} (PID: ${pid})"
            kill -TERM ${pid} 2>/dev/null || true
            sleep 1
            kill -KILL ${pid} 2>/dev/null || true
        fi
    done
    
    # Clean up test data
    if [ -d "${TEST_DATA_DIR}" ]; then
        log_info "Removing test data directory: ${TEST_DATA_DIR}"
        rm -rf "${TEST_DATA_DIR}"
    fi
}

# Trap cleanup on exit
trap cleanup EXIT

# Check binaries exist
check_binaries() {
    log_info "Checking binaries..."
    
    if [ ! -f "${PROJECT_ROOT}/target/release/dl-driver" ]; then
        log_error "dl-driver binary not found. Run: cargo build --release"
        exit 1
    fi
    
    if [ ! -f "${PROJECT_ROOT}/target/release/dl_driver_agent" ]; then
        log_error "dl_driver_agent binary not found. Run: cargo build --release"
        exit 1
    fi
    
    if [ ! -f "${CONFIG_FILE}" ]; then
        log_error "Test config not found: ${CONFIG_FILE}"
        exit 1
    fi
    
    log_success "Binaries and config found"
}

# Start agent processes
start_agents() {
    log_info "Starting ${AGENT_COUNT} agent processes..."
    
    for i in $(seq 0 $((AGENT_COUNT - 1))); do
        local port=$((BASE_PORT + i))
        local agent_id="agent-${i}"
        local log_file="/tmp/dl_driver_agent_${i}.log"
        
        log_info "Starting ${agent_id} on port ${port}"
        
        # Start agent in background
        RUST_LOG=info "${PROJECT_ROOT}/target/release/dl_driver_agent" \
            --port ${port} \
            --agent-id "${agent_id}" \
            > "${log_file}" 2>&1 &
        
        local pid=$!
        log_info "  Agent ${agent_id} started with PID ${pid}, log: ${log_file}"
        
        # Give it a moment to start
        sleep 0.5
    done
    
    # Wait for agents to be ready
    log_info "Waiting for agents to be ready..."
    sleep 2
    
    # Verify agents are listening
    for i in $(seq 0 $((AGENT_COUNT - 1))); do
        local port=$((BASE_PORT + i))
        if ! lsof -ti:${port} >/dev/null 2>&1; then
            log_error "Agent on port ${port} is not listening"
            exit 1
        fi
    done
    
    log_success "All agents started and listening"
}

# Run distributed workload
run_distributed() {
    log_info "Running distributed workload with live stats..."
    log_info "Config: ${CONFIG_FILE}"
    log_info "Test data: ${TEST_DATA_DIR}"
    echo
    
    # Build agent list
    local agents=""
    for i in $(seq 0 $((AGENT_COUNT - 1))); do
        local port=$((BASE_PORT + i))
        if [ -n "$agents" ]; then
            agents="${agents},localhost:${port}"
        else
            agents="localhost:${port}"
        fi
    done
    
    log_info "Agent endpoints: ${agents}"
    log_info "Path template: agent-{id}/"
    log_info "Start delay: 3000ms (validation window)"
    echo
    log_info "==== Watch for startup handshake (READY messages) ===="
    log_info "==== Then live stats updates every 1s ===="
    echo
    
    # Run controller
    cd "${PROJECT_ROOT}"
    RUST_LOG=info ./target/release/dl-driver distributed run \
        --config "${CONFIG_FILE}" \
        --agents "${agents}" \
        --path-template "agent-{id}/" \
        --start-delay-ms 3000
    
    local exit_code=$?
    
    if [ ${exit_code} -ne 0 ]; then
        log_error "Distributed workload failed with exit code ${exit_code}"
        return ${exit_code}
    fi
    
    log_success "Distributed workload completed successfully"
    
    # Find the results directory
    ACTUAL_RESULTS_DIR=$(ls -td dlio-*-distributed_live_stats_test 2>/dev/null | head -1)
    
    if [ -n "$ACTUAL_RESULTS_DIR" ]; then
        log_info "Results directory: ${ACTUAL_RESULTS_DIR}"
    fi
}

# Main execution
main() {
    log_info "==== dl-driver v0.8.7 Live Stats Test (2-agent) ===="
    log_info "Testing:"
    log_info "  - Startup handshake (READY/ERROR validation)"
    log_info "  - Live stats streaming (1s updates)"
    log_info "  - Microsecond precision displays"
    log_info "  - Coordinated start timing"
    log_info "  - Final stats preservation"
    echo
    
    check_binaries
    start_agents
    run_distributed
    
    local exit_code=$?
    
    echo
    if [ ${exit_code} -eq 0 ]; then
        log_success "==== Test COMPLETED ===="
        log_info "Next steps:"
        log_info "  1. Check that startup showed ✅ READY messages"
        log_info "  2. Verify live stats updated every 1s during execution"
        log_info "  3. Confirm latencies shown in µs (not ms)"
        log_info "  4. Check final stats preserved (not overwritten)"
        log_info "  5. Inspect results in: ${ACTUAL_RESULTS_DIR}"
    else
        log_error "==== Test FAILED ===="
    fi
    
    exit ${exit_code}
}

main "$@"
