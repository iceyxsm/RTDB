"""
Batch operations example for RTDB Python SDK.

Demonstrates efficient bulk operations for large datasets.
"""

import asyncio
import random
import time

try:
    from rtdb_sdk.rtdb_python import RtdbClient
except ImportError:
    from rtdb_sdk.client import RtdbClient


async def batch_upsert_demo():
    """Demonstrate batch upsert operations."""
    async with RtdbClient("http://localhost:6333") as client:
        
        if not await client.is_healthy():
            print("Server not available")
            return
        
        # Create collection
        await client.create_collection("batch_demo", dimension=256)
        
        # Generate large batch of vectors
        batch_size = 1000
        print(f"Generating {batch_size} vectors...")
        
        vectors = []
        for i in range(batch_size):
            vec = [random.gauss(0, 1) for _ in range(256)]
            magnitude = sum(x**2 for x in vec) ** 0.5
            vec = [x / magnitude for x in vec]
            
            vectors.append({
                "id": i + 1,
                "vector": vec,
                "payload": {
                    "batch_id": i // 100,
                    "timestamp": time.time()
                }
            })
        
        # Time the upsert operation
        print(f"Upserting {batch_size} vectors...")
        start_time = time.time()
        await client.upsert("batch_demo", vectors)
        elapsed = time.time() - start_time
        
        print(f"Upserted {batch_size} vectors in {elapsed:.2f}s")
        print(f"Rate: {batch_size / elapsed:.0f} vectors/sec")
        
        # Verify
        count = await client.count("batch_demo")
        print(f"Total points in collection: {count}")
        
        # Cleanup
        await client.delete_collection("batch_demo")


async def concurrent_search_demo():
    """Demonstrate concurrent search operations."""
    async with RtdbClient("http://localhost:6333") as client:
        
        if not await client.is_healthy():
            print("Server not available")
            return
        
        await client.create_collection("concurrent_demo", dimension=128)
        
        # Insert some data
        vectors = [
            {"id": i, "vector": [random.random() for _ in range(128)]}
            for i in range(100)
        ]
        await client.upsert("concurrent_demo", vectors)
        
        # Perform concurrent searches
        num_queries = 20
        print(f"\nPerforming {num_queries} concurrent searches...")
        
        async def search_task(task_id):
            query = [random.random() for _ in range(128)]
            results = await client.search(
                "concurrent_demo",
                query,
                limit=10
            )
            return len(results)
        
        start_time = time.time()
        results = await asyncio.gather(*[
            search_task(i) for i in range(num_queries)
        ])
        elapsed = time.time() - start_time
        
        print(f"Completed {num_queries} searches in {elapsed:.2f}s")
        print(f"Rate: {num_queries / elapsed:.0f} queries/sec")
        print(f"Average results per query: {sum(results) / len(results):.1f}")
        
        await client.delete_collection("concurrent_demo")


async def main():
    print("=" * 60)
    print("Batch Operations Demo")
    print("=" * 60)
    
    print("\n--- Batch Upsert Demo ---")
    await batch_upsert_demo()
    
    print("\n--- Concurrent Search Demo ---")
    await concurrent_search_demo()


if __name__ == "__main__":
    asyncio.run(main())
