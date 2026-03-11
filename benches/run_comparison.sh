#!/bin/bash
# Run comprehensive benchmark comparison

set -e

echo "=== RTDB Benchmark Comparison Suite ==="
echo ""

# Build in release mode
echo "Building release binary..."
cargo build --release

# Run all benchmarks
echo ""
echo "Running all benchmarks..."
cargo bench -- --save-baseline main

echo ""
echo "=== Benchmark Summary ==="
echo ""
echo "Results saved to target/criterion/"
echo ""
echo "To compare with baseline:"
echo "  cargo bench -- --baseline main"
