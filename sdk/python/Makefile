.PHONY: build install test clean lint format docs

# Default Python interpreter
PYTHON := python3

# Build the native extension
build:
	maturin build --release

# Install in development mode
develop:
	pip install maturin
	maturin develop

# Install from source
install:
	pip install -e ".[dev]"

# Run all tests
test:
	pytest tests/ -v

# Run integration tests (requires RTDB server)
test-integration:
	pytest tests/test_integration.py -v --integration -s

# Run linting
lint:
	ruff check rtdb_sdk tests
	mypy rtdb_sdk

# Format code
format:
	black rtdb_sdk tests
	ruff check --fix rtdb_sdk tests

# Clean build artifacts
clean:
	rm -rf build/ dist/ *.egg-info .pytest_cache .mypy_cache
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -type f -name "*.pyc" -delete
	cargo clean

# Build documentation
docs:
	cd docs && make html

# Check code quality
check: lint test

# Build wheels for distribution
wheels:
	pip install cibuildwheel
	cibuildwheel --platform linux

# Benchmark
benchmark:
	pytest tests/benchmarks.py -v --benchmark-only
