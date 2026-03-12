# RTDB Python SDK

High-performance Python client for the RTDB vector database.

## Features

- **Native Performance**: Rust-based PyO3 bindings for maximum speed
- **Async Support**: Built-in async/await support with aiohttp fallback
- **Qdrant Compatible**: Drop-in replacement for qdrant-client
- **Type Safe**: Full type hints and mypy support

## Installation

### From PyPI (when published)
```bash
pip install rtdb-sdk
```

### From Source (with native extension)
```bash
cd sdk/python
pip install maturin
maturin develop --release
```

### Pure Python (fallback)
```bash
cd sdk/python
pip install -e .
```

## Quick Start

```python
import asyncio
from rtdb_sdk import RtdbClient

async def main():
    # Connect to RTDB server
    client = RtdbClient("http://localhost:6333", api_key="optional-api-key")
    
    # Check health
    healthy = await client.is_healthy()
    print(f"Server healthy: {healthy}")
    
    # Create collection
    await client.create_collection(
        name="my_collection",
        dimension=128,
        distance="Cosine"
    )
    
    # Upsert points
    points = [
        {"id": 1, "vector": [0.1] * 128, "payload": {"text": "hello"}},
        {"id": 2, "vector": [0.2] * 128, "payload": {"text": "world"}},
    ]
    await client.upsert("my_collection", points)
    
    # Search
    results = await client.search(
        collection_name="my_collection",
        vector=[0.1] * 128,
        limit=10,
        with_payload=True
    )
    
    for result in results:
        print(f"ID: {result.id}, Score: {result.score}")

asyncio.run(main())
```

## Using Context Manager

```python
async with RtdbClient("http://localhost:6333") as client:
    results = await client.search("my_collection", [0.1] * 128)
    # Session automatically closed on exit
```

## Advanced Usage

### Filtering

```python
results = await client.search(
    "my_collection",
    vector=[0.1] * 128,
    filter={
        "must": [
            {"key": "category", "match": {"value": "electronics"}}
        ]
    }
)
```

### Batch Operations

```python
# Efficiently upsert large batches
batch = [{"id": i, "vector": vec} for i, vec in enumerate(vectors)]
await client.upsert("my_collection", batch)
```

## Development

### Running Tests

```bash
pip install -e ".[dev]"
pytest tests/ -v
```

### Building Native Extension

```bash
pip install maturin
maturin develop  # Development build
maturin build --release  # Production wheel
```

## License

MIT License - see LICENSE file for details.
