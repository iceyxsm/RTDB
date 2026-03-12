"""
Integration tests for RTDB Python SDK.

These tests require a running RTDB server.
Run with: pytest tests/test_integration.py -v --integration
"""

import asyncio
import os
import pytest
import random

# Skip all integration tests if RTDB_URL not set
pytestmark = pytest.mark.skipif(
    os.environ.get("RTDB_URL") is None,
    reason="Integration tests require RTDB_URL environment variable"
)

try:
    from rtdb_sdk.rtdb_python import RtdbClient
    NATIVE_AVAILABLE = True
except ImportError:
    from rtdb_sdk.client import RtdbClient
    NATIVE_AVAILABLE = False


@pytest.fixture
async def client():
    """Create a client connected to test server."""
    url = os.environ.get("RTDB_URL", "http://localhost:6333")
    api_key = os.environ.get("RTDB_API_KEY")
    
    async with RtdbClient(url, api_key=api_key) as client:
        yield client


@pytest.fixture
def test_collection():
    """Generate unique test collection name."""
    return f"test_collection_{random.randint(1000, 9999)}"


@pytest.mark.asyncio
@pytest.mark.integration
async def test_full_workflow(client, test_collection):
    """Test complete workflow: create, upsert, search, delete."""
    # Create collection
    created = await client.create_collection(test_collection, dimension=128)
    assert created is True
    
    # Upsert points
    points = [
        {"id": i, "vector": [0.01 * i] * 128, "payload": {"index": i}}
        for i in range(10)
    ]
    status = await client.upsert(test_collection, points)
    assert status == "ok"
    
    # Search
    results = await client.search(
        test_collection,
        vector=[0.0] * 128,
        limit=5,
        with_payload=True
    )
    assert len(results) > 0
    
    # Get specific point
    point = await client.get_point(test_collection, 1)
    assert point.id == 1
    
    # Count points
    count = await client.count(test_collection)
    assert count == 10
    
    # Delete collection
    deleted = await client.delete_collection(test_collection)
    assert deleted is True


@pytest.mark.asyncio
@pytest.mark.integration
async def test_concurrent_operations(client, test_collection):
    """Test concurrent operations."""
    # Create collection
    await client.create_collection(test_collection, dimension=64)
    
    # Concurrent upserts
    async def upsert_batch(start_id):
        points = [
            {"id": start_id + i, "vector": [random.random() for _ in range(64)]}
            for i in range(10)
        ]
        await client.upsert(test_collection, points)
    
    # Run 5 concurrent batches
    await asyncio.gather(*[upsert_batch(i * 10) for i in range(5)])
    
    # Verify count
    count = await client.count(test_collection)
    assert count == 50
    
    # Cleanup
    await client.delete_collection(test_collection)


@pytest.mark.asyncio
@pytest.mark.integration
async def test_large_vectors(client, test_collection):
    """Test with large vectors."""
    await client.create_collection(test_collection, dimension=512)
    
    # Upsert large vector
    points = [{"id": 1, "vector": [0.1] * 512}]
    await client.upsert(test_collection, points)
    
    # Search with large vector
    results = await client.search(test_collection, [0.1] * 512, limit=1)
    assert len(results) == 1
    assert results[0].id == 1
    
    await client.delete_collection(test_collection)


@pytest.mark.asyncio
@pytest.mark.integration
async def test_payload_operations(client, test_collection):
    """Test payload handling."""
    await client.create_collection(test_collection, dimension=32)
    
    # Upsert with complex payload
    points = [{
        "id": 1,
        "vector": [0.1] * 32,
        "payload": {
            "text": "Hello World",
            "tags": ["tag1", "tag2"],
            "metadata": {"source": "test", "version": 1.0}
        }
    }]
    await client.upsert(test_collection, points)
    
    # Retrieve and verify payload
    point = await client.get_point(test_collection, 1)
    assert point.id == 1
    assert point.payload is not None
    
    await client.delete_collection(test_collection)
