#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

#
# test_distributed_local.sh - Local distributed execution test
#
# Tests dl-driver distributed histogram aggregation with 2 agents on localhost.
# Similar to sai3-bench/scripts/local_docker_test.sh pattern.
#
# Requirements:
#   - dl-driver and dl_driver_agent binaries in target/release/
#   - Available ports 50051-50052 for agents
#   - /tmp space for test data
#

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
AGENT_COUNT=2
BASE_PORT=50051
TEST_DATA_DIR="/tmp/dlio_distributed_test"
RESULTS_DIR="${PROJECT_ROOT}/distributed-test-results"
CONFIG_FILE="${PROJECT_ROOT}/tests/dlio_configs/minimal_config.yaml"
TEST_CONFIG="/tmp/dlio_distributed_test_config.yaml"

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

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
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
    
    if [ -f "${TEST_CONFIG}" ]; then
        rm -f "${TEST_CONFIG}"
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
    
    log_success "Binaries found"
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
        log_info "  Agent ${agent_id} started with PID ${pid}"
        
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

# Generate test configuration
generate_config() {
    log_info "Generating test configuration..."
    
    # Create a modified config that uses agent-specific paths
    # Each agent will write to ${TEST_DATA_DIR}/agent-{id}/
    cat > "${TEST_CONFIG}" <<EOF
# Auto-generated distributed test configuration
model:
  name: distributed_test

framework: pytorch

workflow:
  generate_data: true
  train: true
  checkpoint: false

dataset:
  data_folder: file://${TEST_DATA_DIR}
  format: npz
  num_files_train: 100
  record_length_bytes: 1048576

reader:
  data_loader: pytorch
  batch_size: 16
  read_threads: 4
  compute_threads: 2
  prefetch: 8
  shuffle: true
EOF
    
    log_success "Test configuration generated: ${TEST_CONFIG}"
}

# Run distributed workload
run_distributed() {
    log_info "Running distributed workload..."
    
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
    log_info "Start delay: 1000ms"
    
    # Run controller (it creates its own results directory with timestamp)
    cd "${PROJECT_ROOT}"
    RUST_LOG=info ./target/release/dl-driver distributed run \
        --config "${TEST_CONFIG}" \
        --agents "${agents}" \
        --path-template "agent-{id}/" \
        --start-delay-ms 1000 \
        2>&1 | tee "${RESULTS_DIR}/controller.log"
    
    local exit_code=${PIPESTATUS[0]}
    
    if [ ${exit_code} -ne 0 ]; then
        log_error "Distributed workload failed with exit code ${exit_code}"
        return ${exit_code}
    fi
    
    # Find the actual results directory created by the controller
    ACTUAL_RESULTS_DIR=$(ls -td dlio-*-dlio_distributed_test_config 2>/dev/null | head -1)
    
    if [ -z "$ACTUAL_RESULTS_DIR" ]; then
        log_error "Could not find results directory"
        return 1
    fi
    
    log_info "Results directory: ${ACTUAL_RESULTS_DIR}"
    
    log_success "Distributed workload completed successfully"
}

# Verify results
verify_results() {
    log_info "Verifying results..."
    
    if [ -z "$ACTUAL_RESULTS_DIR" ] || [ ! -d "$ACTUAL_RESULTS_DIR" ]; then
        log_error "Results directory not found: ${ACTUAL_RESULTS_DIR}"
        return 1
    fi
    
    local errors=0
    
    # Check consolidated histogram TSV exists
    if [ ! -f "${ACTUAL_RESULTS_DIR}/consolidated_storage_results.tsv" ]; then
        log_error "Consolidated storage_results.tsv not found"
        ((errors++))
    else
        log_success "Found consolidated_storage_results.tsv"
        
        # Check it has content
        local line_count=$(wc -l < "${ACTUAL_RESULTS_DIR}/consolidated_storage_results.tsv")
        log_info "  Lines: ${line_count}"
        
        if [ ${line_count} -lt 2 ]; then
            log_error "  consolidated_storage_results.tsv has insufficient data"
            ((errors++))
        fi
        
        # Check for bucket-level detail (should have rows with bucket_idx 4,5,98,99)
        if ! grep -q "bucket_idx" "${ACTUAL_RESULTS_DIR}/consolidated_storage_results.tsv"; then
            log_error "  Missing bucket_idx column"
            ((errors++))
        else
            log_success "  Contains bucket-level histogram data"
        fi
    fi
    
    # Check per-agent TSV files exist
    for i in $(seq 0 $((AGENT_COUNT - 1))); do
        local agent_tsv="${ACTUAL_RESULTS_DIR}/agents/agent-${i}/storage_results.tsv"
        if [ ! -f "${agent_tsv}" ]; then
            log_error "Agent ${i} TSV not found: ${agent_tsv}"
            ((errors++))
        else
            log_success "Found agent-${i}/storage_results.tsv"
            
            # Check it has content
            local line_count=$(wc -l < "${agent_tsv}")
            log_info "  Lines: ${line_count}"
            
            if [ ${line_count} -lt 2 ]; then
                log_error "  agent-${i} TSV has insufficient data"
                ((errors++))
            fi
            
            # Verify bucket-level format
            if ! grep -q "bucket_idx" "${agent_tsv}"; then
                log_error "  agent-${i} TSV missing bucket_idx column"
                ((errors++))
            fi
        fi
    done
    
    # Display sample of consolidated TSV
    if [ -f "${ACTUAL_RESULTS_DIR}/consolidated_storage_results.tsv" ]; then
        log_info "Sample of consolidated histogram TSV:"
        head -10 "${ACTUAL_RESULTS_DIR}/consolidated_storage_results.tsv" | sed 's/^/  /'
        echo
    fi
    
    # Display sample of agent-0 TSV
    if [ -f "${ACTUAL_RESULTS_DIR}/agents/agent-0/storage_results.tsv" ]; then
        log_info "Sample of agent-0 histogram TSV:"
        head -10 "${ACTUAL_RESULTS_DIR}/agents/agent-0/storage_results.tsv" | sed 's/^/  /'
        echo
    fi
    
    # Check console.log
    if [ -f "${ACTUAL_RESULTS_DIR}/console.log" ]; then
        log_success "Found console.log"
        log_info "Console.log contents:"
        cat "${ACTUAL_RESULTS_DIR}/console.log" | sed 's/^/  /'
        echo
    fi
    
    if [ ${errors} -eq 0 ]; then
        log_success "All verification checks passed!"
        return 0
    else
        log_error "${errors} verification check(s) failed"
        return 1
    fi
}

# Main execution
main() {
    log_info "==== dl-driver Distributed Execution Test ===="
    log_info "Agent count: ${AGENT_COUNT}"
    log_info "Base port: ${BASE_PORT}"
    log_info "Test data: ${TEST_DATA_DIR}"
    log_info "Results: ${RESULTS_DIR}"
    echo
    
    check_binaries
    start_agents
    generate_config
    run_distributed
    
    echo
    log_info "==== Verification ===="
    verify_results
    
    local verify_exit=$?
    
    echo
    if [ ${verify_exit} -eq 0 ]; then
        log_success "==== Test PASSED ===="
    else
        log_error "==== Test FAILED ===="
    fi
    
    exit ${verify_exit}
}

main "$@"
