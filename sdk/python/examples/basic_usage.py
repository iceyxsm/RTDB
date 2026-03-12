"""
Basic usage example for RTDB Python SDK.

This example demonstrates:
- Connecting to RTDB server
- Creating collections
- Upserting vectors
- Searching for similar vectors
"""

import asyncio
import random

# Try native extension first, fallback to pure Python
try:
    from rtdb_sdk.rtdb_python import RtdbClient
    print("Using native Rust extension")
except ImportError:
    from rtdb_sdk.client import RtdbClient
    print("Using pure Python implementation")


async def main():
    # Connect to RTDB (default: http://localhost:6333)
    async with RtdbClient("http://localhost:6333") as client:
        
        # Check if server is healthy
        healthy = await client.is_healthy()
        print(f"Server healthy: {healthy}")
        
        if not healthy:
            print("Server is not responding. Make sure RTDB is running.")
            return
        
        # Create a collection for 128-dimensional vectors
        print("\nCreating collection 'documents'...")
        await client.create_collection(
            name="documents",
            dimension=128,
            distance="Cosine"  # or "Euclidean", "Dot"
        )
        print("Collection created!")
        
        # Generate and upsert some sample vectors
        print("\nUpserting sample vectors...")
        num_vectors = 100
        vectors = []
        
        for i in range(num_vectors):
            # Generate random normalized vector
            vec = [random.gauss(0, 1) for _ in range(128)]
            magnitude = sum(x**2 for x in vec) ** 0.5
            vec = [x / magnitude for x in vec]  # Normalize
            
            vectors.append({
                "id": i + 1,
                "vector": vec,
                "payload": {
                    "index": i,
                    "category": random.choice(["tech", "science", "art"]),
                    "score": random.random() * 100
                }
            })
        
        await client.upsert("documents", vectors)
        print(f"Upserted {num_vectors} vectors")
        
        # Search for similar vectors
        print("\nSearching for similar vectors...")
        query_vector = vectors[0]["vector"]  # Use first vector as query
        
        results = await client.search(
            collection_name="documents",
            vector=query_vector,
            limit=5,
            with_payload=True
        )
        
        print("\nTop 5 similar vectors:")
        for result in results:
            print(f"  ID: {result.id}, Score: {result.score:.4f}")
            if hasattr(result, 'payload') and result.payload:
                print(f"    Payload: {result.payload}")
        
        # Get a specific point
        print("\nRetrieving point ID 10...")
        point = await client.get_point("documents", 10)
        print(f"Point ID: {point.id}, Vector length: {len(point.vector)}")
        
        # Count points
        count = await client.count("documents")
        print(f"\nTotal points in collection: {count}")
        
        # List collections
        collections = await client.list_collections()
        print(f"\nCollections: {collections}")
        
        # Cleanup
        print("\nCleaning up...")
        await client.delete_collection("documents")
        print("Collection deleted!")


if __name__ == "__main__":
    asyncio.run(main())
