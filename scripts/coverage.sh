#!/bin/bash
set -e

echo "Running tests with coverage..."

cargo llvm-cov --all-features --workspace --html

echo ""
echo "Coverage report generated at: target/llvm-cov/html/index.html"
echo ""
echo "To fail if coverage is below 95%, run:"
echo "  cargo llvm-cov --fail-uncovered-lines 95"
