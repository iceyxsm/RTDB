#!/bin/bash
# Run gRPC Benchmarks Script
# 
# This script checks for protoc and runs the gRPC benchmarks
# Usage: ./scripts/run-grpc-benchmarks.sh [options]

set -e

echo "=== RTDB gRPC Benchmark Runner ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if protoc is installed
check_protoc() {
    if command -v protoc &> /dev/null; then
        PROTOC_VERSION=$(protoc --version)
        echo -e "${GREEN} protoc found: $PROTOC_VERSION${NC}"
        return 0
    else
        echo -e "${RED} protoc not found${NC}"
        return 1
    fi
}

# Install protoc on different platforms
install_protoc() {
    echo -e "${YELLOW}Installing protoc...${NC}"
    
    case "$(uname -s)" in
        Linux*)
            if command -v apt-get &> /dev/null; then
                # Debian/Ubuntu
                sudo apt-get update
                sudo apt-get install -y protobuf-compiler
            elif command -v yum &> /dev/null; then
                # RHEL/CentOS/Fedora
                sudo yum install -y protobuf-compiler
            elif command -v pacman &> /dev/null; then
                # Arch Linux
                sudo pacman -S protobuf
            else
                echo -e "${RED}Unsupported package manager. Please install protoc manually.${NC}"
                echo "Visit: https://grpc.io/docs/protoc-installation/"
                exit 1
            fi
            ;;
        Darwin*)
            # macOS
            if command -v brew &> /dev/null; then
                brew install protobuf
            else
                echo -e "${RED}Homebrew not found. Please install protoc manually.${NC}"
                echo "Visit: https://grpc.io/docs/protoc-installation/"
                exit 1
            fi
            ;;
        MINGW*|CYGWIN*|MSYS*)
            # Windows
            echo -e "${YELLOW}Windows detected. Please install protoc manually:${NC}"
            echo "1. Download from: https://github.com/protocolbuffers/protobuf/releases"
            echo "2. Extract to C:\protobuf"
            echo "3. Add C:\protobuf\bin to your PATH"
            exit 1
            ;;
        *)
            echo -e "${RED}Unsupported platform. Please install protoc manually.${NC}"
            echo "Visit: https://grpc.io/docs/protoc-installation/"
            exit 1
            ;;
    esac
}

# Run benchmarks
run_benchmarks() {
    echo ""
    echo -e "${GREEN}=== Running gRPC Benchmarks ===${NC}"
    echo ""
    
    # Check if cargo-criterion is installed for better output
    if cargo criterion --version &> /dev/null; then
        echo "Using cargo-criterion for enhanced output..."
        cargo criterion --bench grpc_benchmark --features grpc "$@"
    else
        echo "Running with cargo bench..."
        echo "Tip: Install cargo-criterion for better output: cargo install cargo-criterion"
        cargo bench --bench grpc_benchmark --features grpc "$@"
    fi
}

# Parse arguments
BENCHMARK_FILTER=""
INSTALL_PROTOC=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --install)
            INSTALL_PROTOC=true
            shift
            ;;
        --filter)
            BENCHMARK_FILTER="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --install          Install protoc if not found"
            echo "  --filter PATTERN   Only run benchmarks matching PATTERN"
            echo "  --help             Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                                    # Run all benchmarks"
            echo "  $0 --install                          # Install protoc and run"
            echo "  $0 --filter connection_pooling        # Run only connection pooling benchmarks"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main logic
cd "$(dirname "$0")/.."

if ! check_protoc; then
    if [ "$INSTALL_PROTOC" = true ]; then
        install_protoc
        check_protoc || exit 1
    else
        echo ""
        echo -e "${YELLOW}protoc is required to run gRPC benchmarks.${NC}"
        echo "Run with --install to install automatically, or install manually:"
        echo "  https://grpc.io/docs/protoc-installation/"
        exit 1
    fi
fi

# Run benchmarks
if [ -n "$BENCHMARK_FILTER" ]; then
    run_benchmarks -- "$BENCHMARK_FILTER"
else
    run_benchmarks
fi

echo ""
echo -e "${GREEN}=== Benchmarks Complete ===${NC}"
echo "Results saved to: target/criterion/"
