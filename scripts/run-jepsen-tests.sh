#!/bin/bash
# RTDB Jepsen Testing Script
# Production-grade distributed systems correctness validation

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
JEPSEN_CONFIG="${PROJECT_ROOT}/config/jepsen.yaml"
RESULTS_DIR="${PROJECT_ROOT}/jepsen-results"
LOG_FILE="${RESULTS_DIR}/jepsen-test.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
DEFAULT_NODES="http://localhost:6333,http://localhost:6334,http://localhost:6335"
DEFAULT_DURATION=300
DEFAULT_CLIENTS=8
ENABLE_SIMDX=true
VERBOSE=false

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to show usage
show_usage() {
    cat << EOF
RTDB Jepsen Testing Script

Usage: $0 [OPTIONS] COMMAND

Commands:
    health              Validate cluster health
    linearizability     Run linearizability test
    bank-transfer       Run bank transfer consistency test
    register           Run register consistency test
    partition          Run partition tolerance test
    suite              Run comprehensive test suite
    quick              Run quick validation (5 minutes)
    stress             Run stress test (2 hours)

Options:
    -n, --nodes NODES          Comma-separated list of node endpoints
                              (default: $DEFAULT_NODES)
    -d, --duration SECONDS     Test duration in seconds (default: $DEFAULT_DURATION)
    -c, --clients COUNT        Number of concurrent clients (default: $DEFAULT_CLIENTS)
    --no-simdx                Disable SIMDX optimizations
    -v, --verbose             Enable verbose logging
    -h, --help                Show this help message

Examples:
    # Quick health check
    $0 health

    # Run linearizability test for 5 minutes
    $0 -d 300 linearizability

    # Run full test suite with custom nodes
    $0 -n "http://node1:6333,http://node2:6333,http://node3:6333" suite

    # Run stress test with verbose output
    $0 -v stress

EOF
}

# Function to parse command line arguments
parse_args() {
    NODES="$DEFAULT_NODES"
    DURATION="$DEFAULT_DURATION"
    CLIENTS="$DEFAULT_CLIENTS"
    COMMAND=""

    while [[ $# -gt 0 ]]; do
        case $1 in
            -n|--nodes)
                NODES="$2"
                shift 2
                ;;
            -d|--duration)
                DURATION="$2"
                shift 2
                ;;
            -c|--clients)
                CLIENTS="$2"
                shift 2
                ;;
            --no-simdx)
                ENABLE_SIMDX=false
                shift
                ;;
            -v|--verbose)
                VERBOSE=true
                shift
                ;;
            -h|--help)
                show_usage
                exit 0
                ;;
            health|linearizability|bank-transfer|register|partition|suite|quick|stress)
                COMMAND="$1"
                shift
                ;;
            *)
                print_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
        esac
    done

    if [[ -z "$COMMAND" ]]; then
        print_error "No command specified"
        show_usage
        exit 1
    fi
}

# Function to setup environment
setup_environment() {
    print_status "Setting up Jepsen test environment..."
    
    # Create results directory
    mkdir -p "$RESULTS_DIR"
    
    # Initialize log file
    echo "RTDB Jepsen Test Run - $(date)" > "$LOG_FILE"
    
    # Build Jepsen binary if needed
    if [[ ! -f "${PROJECT_ROOT}/target/release/rtdb-jepsen" ]]; then
        print_status "Building Jepsen test binary..."
        cd "$PROJECT_ROOT"
        cargo build --release --bin rtdb-jepsen
    fi
    
    print_success "Environment setup complete"
}

# Function to validate cluster health
validate_cluster_health() {
    print_status "Validating cluster health..."
    
    local node_array
    IFS=',' read -ra node_array <<< "$NODES"
    
    for node in "${node_array[@]}"; do
        print_status "Checking node: $node"
        
        # Extract host and port from URL
        local host_port
        host_port=$(echo "$node" | sed 's|http[s]*://||' | cut -d'/' -f1)
        local host
        host=$(echo "$host_port" | cut -d':' -f1)
        local port
        port=$(echo "$host_port" | cut -d':' -f2)
        
        # Check if node is reachable
        if ! timeout 5 bash -c "</dev/tcp/$host/$port"; then
            print_error "Node $node is not reachable"
            return 1
        fi
        
        # Check health endpoint
        local health_port=$((port + 2747)) # Assuming health port is REST port + offset
        if command -v curl >/dev/null 2>&1; then
            if ! curl -s --max-time 5 "http://$host:$health_port/health" >/dev/null; then
                print_warning "Health endpoint not available for $node"
            fi
        fi
    done
    
    print_success "All nodes are reachable"
}

# Function to run Jepsen command
run_jepsen_command() {
    local cmd="$1"
    shift
    local args=("$@")
    
    local jepsen_cmd="${PROJECT_ROOT}/target/release/rtdb-jepsen"
    local common_args=()
    
    # Add common arguments
    if [[ "$ENABLE_SIMDX" == "true" ]]; then
        common_args+=(--enable-simdx)
    fi
    
    if [[ "$VERBOSE" == "true" ]]; then
        common_args+=(--verbose)
    fi
    
    # Convert comma-separated nodes to multiple --nodes arguments
    local node_args=()
    IFS=',' read -ra node_array <<< "$NODES"
    for node in "${node_array[@]}"; do
        node_args+=(--nodes "$node")
    done
    
    print_status "Running Jepsen command: $cmd"
    print_status "Nodes: $NODES"
    print_status "Duration: ${DURATION}s, Clients: $CLIENTS, SIMDX: $ENABLE_SIMDX"
    
    # Run the command and capture output
    local start_time
    start_time=$(date +%s)
    
    if "$jepsen_cmd" "${common_args[@]}" "$cmd" "${node_args[@]}" "${args[@]}" 2>&1 | tee -a "$LOG_FILE"; then
        local end_time
        end_time=$(date +%s)
        local duration=$((end_time - start_time))
        print_success "Jepsen test completed successfully in ${duration}s"
        return 0
    else
        local end_time
        end_time=$(date +%s)
        local duration=$((end_time - start_time))
        print_error "Jepsen test failed after ${duration}s"
        return 1
    fi
}

# Function to run health check
run_health_check() {
    print_status "Running cluster health validation..."
    validate_cluster_health
    run_jepsen_command "health"
}

# Function to run linearizability test
run_linearizability_test() {
    print_status "Running linearizability test..."
    run_jepsen_command "linearizability" \
        --clients "$CLIENTS" \
        --duration "$DURATION" \
        --rate 100 \
        --partition-prob 0.1 \
        --timeout-ms 5000
}

# Function to run bank transfer test
run_bank_transfer_test() {
    print_status "Running bank transfer consistency test..."
    run_jepsen_command "bank-transfer" \
        --accounts 10 \
        --initial-balance 1000 \
        --duration "$DURATION"
}

# Function to run register test
run_register_test() {
    print_status "Running register consistency test..."
    run_jepsen_command "register" \
        --registers 20 \
        --duration "$DURATION"
}

# Function to run partition tolerance test
run_partition_test() {
    print_status "Running partition tolerance test..."
    run_jepsen_command "linearizability" \
        --clients "$CLIENTS" \
        --duration "$DURATION" \
        --rate 50 \
        --partition-prob 0.3 \
        --timeout-ms 10000
}

# Function to run comprehensive test suite
run_test_suite() {
    print_status "Running comprehensive Jepsen test suite..."
    
    local suite_duration=$((DURATION / 5)) # Divide duration among tests
    if [[ $suite_duration -lt 60 ]]; then
        suite_duration=60 # Minimum 1 minute per test
    fi
    
    run_jepsen_command "suite" \
        --duration-per-test "$suite_duration"
}

# Function to run quick validation
run_quick_test() {
    print_status "Running quick Jepsen validation (5 minutes)..."
    
    local quick_duration=60 # 1 minute per test
    DURATION=60
    CLIENTS=4
    
    print_status "1/5: Health check"
    run_health_check || return 1
    
    print_status "2/5: Quick linearizability test"
    run_jepsen_command "linearizability" \
        --clients "$CLIENTS" \
        --duration "$quick_duration" \
        --rate 50 \
        --partition-prob 0.05 || return 1
    
    print_status "3/5: Quick register test"
    run_jepsen_command "register" \
        --registers 5 \
        --duration "$quick_duration" || return 1
    
    print_status "4/5: Quick partition test"
    run_jepsen_command "linearizability" \
        --clients "$CLIENTS" \
        --duration "$quick_duration" \
        --rate 30 \
        --partition-prob 0.2 || return 1
    
    print_status "5/5: Quick bank transfer test"
    run_jepsen_command "bank-transfer" \
        --accounts 5 \
        --initial-balance 100 \
        --duration "$quick_duration" || return 1
    
    print_success "Quick validation completed successfully!"
}

# Function to run stress test
run_stress_test() {
    print_status "Running Jepsen stress test (2 hours)..."
    
    DURATION=7200 # 2 hours
    CLIENTS=16
    
    print_status "Starting 2-hour stress test with high load..."
    run_jepsen_command "suite" \
        --duration-per-test 1440 # 24 minutes per test
}

# Function to generate summary report
generate_summary() {
    print_status "Generating test summary..."
    
    local summary_file="${RESULTS_DIR}/summary.txt"
    
    cat > "$summary_file" << EOF
RTDB Jepsen Test Summary
========================

Test Run: $(date)
Command: $COMMAND
Nodes: $NODES
Duration: ${DURATION}s
Clients: $CLIENTS
SIMDX Enabled: $ENABLE_SIMDX

Results:
$(tail -20 "$LOG_FILE")

Full log available at: $LOG_FILE
EOF

    print_success "Summary generated: $summary_file"
}

# Main execution
main() {
    print_status "RTDB Jepsen Testing Framework"
    print_status "=============================="
    
    parse_args "$@"
    setup_environment
    
    # Validate cluster health first (except for health command)
    if [[ "$COMMAND" != "health" ]]; then
        validate_cluster_health || {
            print_error "Cluster health validation failed"
            exit 1
        }
    fi
    
    # Execute the requested command
    case "$COMMAND" in
        health)
            run_health_check
            ;;
        linearizability)
            run_linearizability_test
            ;;
        bank-transfer)
            run_bank_transfer_test
            ;;
        register)
            run_register_test
            ;;
        partition)
            run_partition_test
            ;;
        suite)
            run_test_suite
            ;;
        quick)
            run_quick_test
            ;;
        stress)
            run_stress_test
            ;;
        *)
            print_error "Unknown command: $COMMAND"
            exit 1
            ;;
    esac
    
    local exit_code=$?
    
    # Generate summary
    generate_summary
    
    if [[ $exit_code -eq 0 ]]; then
        print_success "All Jepsen tests completed successfully!"
        print_status "No consistency violations detected - RTDB cluster is correct"
    else
        print_error "Jepsen tests failed!"
        print_error "Consistency violations detected - review logs for details"
    fi
    
    exit $exit_code
}

# Run main function with all arguments
main "$@"