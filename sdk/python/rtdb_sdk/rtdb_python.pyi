"""
Type stubs for RTDB Python native extension.

This file provides type hints for the Rust-based native module.
"""

from typing import List, Dict, Optional, Any, Callable
import asyncio


class Point:
    """A point in the vector database."""
    
    id: int
    vector: List[float]
    payload: Optional[Dict[str, Any]]
    
    def __init__(
        self,
        id: int,
        vector: List[float],
        payload: Optional[Dict[str, Any]] = None
    ) -> None: ...
    
    def __repr__(self) -> str: ...


class SearchResult:
    """Result from a vector search."""
    
    id: int
    score: float
    payload: Optional[Dict[str, Any]]
    
    def __init__(
        self,
        id: int,
        score: float,
        payload: Optional[Dict[str, Any]] = None
    ) -> None: ...
    
    def __repr__(self) -> str: ...


class RtdbClient:
    """
    Async client for RTDB vector database.
    
    This client provides high-performance access to RTDB via native Rust bindings.
    All methods are async and should be awaited.
    
    Example:
        >>> async with RtdbClient("http://localhost:6333") as client:
        ...     await client.create_collection("docs", dimension=128)
        ...     results = await client.search("docs", [0.1] * 128)
    """
    
    base_url: str
    api_key: Optional[str]
    
    def __init__(
        self,
        url: str = "http://localhost:6333",
        api_key: Optional[str] = None
    ) -> None: ...
    
    def __aenter__(self) -> "RtdbClient": ...
    def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None: ...
    
    async def is_healthy(self) -> bool:
        """Check if the RTDB server is healthy."""
        ...
    
    async def create_collection(
        self,
        name: str,
        dimension: int,
        distance: str = "Cosine"
    ) -> bool:
        """
        Create a new collection.
        
        Args:
            name: Collection name
            dimension: Vector dimension
            distance: Distance metric (Cosine, Euclidean, Dot)
        
        Returns:
            True if successful
        """
        ...
    
    async def delete_collection(self, name: str) -> bool:
        """Delete a collection."""
        ...
    
    async def list_collections(self) -> List[str]:
        """List all collections."""
        ...
    
    async def upsert(
        self,
        collection_name: str,
        points: List[Dict[str, Any]]
    ) -> str:
        """
        Upsert points into a collection.
        
        Args:
            collection_name: Target collection
            points: List of points with 'id', 'vector', and optional 'payload'
        
        Returns:
            Operation status
        """
        ...
    
    async def search(
        self,
        collection_name: str,
        vector: List[float],
        limit: int = 10,
        with_payload: bool = True,
        filter: Optional[Dict[str, Any]] = None
    ) -> List[SearchResult]:
        """
        Search for similar vectors.
        
        Args:
            collection_name: Collection to search
            vector: Query vector
            limit: Maximum results
            with_payload: Include payload in results
            filter: Optional filter conditions
        
        Returns:
            List of search results sorted by similarity score
        """
        ...
    
    async def get_point(self, collection_name: str, point_id: int) -> Point:
        """Get a point by ID."""
        ...
    
    async def delete_point(self, collection_name: str, point_id: int) -> bool:
        """Delete a point by ID."""
        ...
    
    async def count(self, collection_name: str) -> int:
        """Count points in a collection."""
        ...
