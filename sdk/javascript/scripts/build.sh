#!/bin/bash

# RTDB JavaScript SDK Build Script

set -e

echo "Building RTDB JavaScript SDK..."

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
    echo "Node.js is not installed. Please install Node.js 16+ to continue."
    exit 1
fi

# Check Node.js version
NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 16 ]; then
    echo "Node.js version 16+ is required. Current version: $(node -v)"
    exit 1
fi

echo "Node.js version: $(node -v)"

# Install dependencies
echo "Installing dependencies..."
npm install

# Type checking
echo "Type checking..."
npm run type-check

# Linting
echo "Linting..."
npm run lint

# Build
echo "Building..."
npm run build

# Run tests
echo "Running tests..."
npm test

# Check build outputs
echo "Checking build outputs..."
if [ -f "dist/index.js" ] && [ -f "dist/index.d.ts" ] && [ -f "dist/index.esm.js" ]; then
    echo "All build outputs generated successfully"
    
    # Show file sizes
    echo "Build output sizes:"
    ls -lh dist/
else
    echo "Build outputs missing"
    exit 1
fi

# Generate documentation (if typedoc is available)
if command -v typedoc &> /dev/null; then
    echo "Generating documentation..."
    npm run docs
else
    echo "TypeDoc not available, skipping documentation generation"
fi

echo "Build completed successfully!"
echo ""
echo "Package ready for publishing:"
echo "   - CommonJS: dist/index.js"
echo "   - ES Module: dist/index.esm.js"
echo "   - UMD: dist/rtdb-client.umd.js"
echo "   - TypeScript: dist/index.d.ts"
echo ""
echo "To publish: npm publish"