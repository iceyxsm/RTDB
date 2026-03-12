"""
RTDB Python SDK

A high-performance Python client for the RTDB vector database.

Example usage:
    >>> import asyncio
    >>> from rtdb_sdk import RtdbClient
    >>> 
    >>> async def main():
    ...     async with RtdbClient("http://localhost:6333") as client:
    ...         # Create collection
    ...         await client.create_collection("my_collection", dimension=128)
    ...         
    ...         # Upsert vectors
    ...         await client.upsert("my_collection", [
    ...             {"id": 1, "vector": [0.1] * 128}
    ...         ])
    ...         
    ...         # Search
    ...         results = await client.search(
    ...             "my_collection", [0.1] * 128, limit=10
    ...         )
    ...         for r in results:
    ...             print(f"ID: {r.id}, Score: {r.score}")
    ... 
    >>> asyncio.run(main())
"""

from importlib.util import find_spec

# Try to import the native extension first
_NATIVE_AVAILABLE = find_spec("rtdb_sdk.rtdb_python") is not None

if _NATIVE_AVAILABLE:
    from rtdb_sdk.rtdb_python import (
        RtdbClient,
        Point,
        SearchResult,
    )
else:
    # Fallback to pure Python implementation
    import warnings
    warnings.warn(
        "RTDB native extension not found. Using pure Python implementation. "
        "For better performance, install the native extension with: "
        "pip install rtdb-sdk",
        UserWarning,
        stacklevel=2
    )
    from rtdb_sdk.client import RtdbClient, Point, SearchResult

__version__ = "0.1.0"
__all__ = ["RtdbClient", "Point", "SearchResult", "__version__"]
