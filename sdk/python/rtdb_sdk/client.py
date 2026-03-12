"""
Pure Python fallback implementation of the RTDB client.
Used when the native Rust extension is not available.
"""

import asyncio
import aiohttp
from typing import List, Dict, Optional, Any
from dataclasses import dataclass


@dataclass
class Point:
    """A point in the vector database."""
    id: int
    vector: List[float]
    payload: Optional[Dict[str, Any]] = None


@dataclass
class SearchResult:
    """Result from a vector search."""
    id: int
    score: float
    payload: Optional[Dict[str, Any]] = None


class RtdbClient:
    """
    Async client for RTDB vector database.
    
    Args:
        url: Base URL of the RTDB server
        api_key: Optional API key for authentication
        timeout: Request timeout in seconds
    """
    
    def __init__(
        self,
        url: str = "http://localhost:6333",
        api_key: Optional[str] = None,
        timeout: float = 30.0
    ):
        self.base_url = url.rstrip('/')
        self.api_key = api_key
        self.timeout = aiohttp.ClientTimeout(total=timeout)
        self._session: Optional[aiohttp.ClientSession] = None
    
    async def __aenter__(self):
        self._session = aiohttp.ClientSession(timeout=self.timeout)
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self._session:
            await self._session.close()
            self._session = None
    
    def _get_session(self) -> aiohttp.ClientSession:
        if self._session is None:
            self._session = aiohttp.ClientSession(timeout=self.timeout)
        return self._session
    
    def _get_headers(self) -> Dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["X-API-Key"] = self.api_key
        return headers
    
    async def is_healthy(self) -> bool:
        """Check if the server is healthy."""
        try:
            session = self._get_session()
            async with session.get(
                f"{self.base_url}/healthz",
                headers=self._get_headers()
            ) as response:
                return response.status == 200
        except Exception:
            return False
    
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
        session = self._get_session()
        async with session.put(
            f"{self.base_url}/collections/{name}",
            headers=self._get_headers(),
            json={"dimension": dimension, "distance": distance}
        ) as response:
            if response.status == 200:
                return True
            text = await response.text()
            raise RuntimeError(f"Failed to create collection: {text}")
    
    async def delete_collection(self, name: str) -> bool:
        """Delete a collection."""
        session = self._get_session()
        async with session.delete(
            f"{self.base_url}/collections/{name}",
            headers=self._get_headers()
        ) as response:
            if response.status == 200:
                return True
            text = await response.text()
            raise RuntimeError(f"Failed to delete collection: {text}")
    
    async def list_collections(self) -> List[str]:
        """List all collections."""
        session = self._get_session()
        async with session.get(
            f"{self.base_url}/collections",
            headers=self._get_headers()
        ) as response:
            if response.status == 200:
                data = await response.json()
                return [c["name"] for c in data["result"]["collections"]]
            text = await response.text()
            raise RuntimeError(f"Failed to list collections: {text}")
    
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
        session = self._get_session()
        async with session.put(
            f"{self.base_url}/collections/{collection_name}/points",
            headers=self._get_headers(),
            json={"points": points}
        ) as response:
            if response.status == 200:
                data = await response.json()
                return data["result"]["status"]
            text = await response.text()
            raise RuntimeError(f"Failed to upsert points: {text}")
    
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
            List of search results
        """
        body = {
            "vector": vector,
            "limit": limit,
            "with_payload": with_payload
        }
        if filter:
            body["filter"] = filter
        
        session = self._get_session()
        async with session.post(
            f"{self.base_url}/collections/{collection_name}/points/search",
            headers=self._get_headers(),
            json=body
        ) as response:
            if response.status == 200:
                data = await response.json()
                return [
                    SearchResult(
                        id=p["id"],
                        score=p["score"],
                        payload=p.get("payload")
                    )
                    for p in data["result"]
                ]
            text = await response.text()
            raise RuntimeError(f"Failed to search: {text}")
    
    async def get_point(self, collection_name: str, point_id: int) -> Point:
        """Get a point by ID."""
        session = self._get_session()
        async with session.get(
            f"{self.base_url}/collections/{collection_name}/points/{point_id}",
            headers=self._get_headers()
        ) as response:
            if response.status == 200:
                data = await response.json()
                result = data["result"]
                return Point(
                    id=result["id"],
                    vector=result["vector"],
                    payload=result.get("payload")
                )
            text = await response.text()
            raise RuntimeError(f"Failed to get point: {text}")
    
    async def delete_point(self, collection_name: str, point_id: int) -> bool:
        """Delete a point by ID."""
        session = self._get_session()
        async with session.delete(
            f"{self.base_url}/collections/{collection_name}/points/{point_id}",
            headers=self._get_headers()
        ) as response:
            if response.status == 200:
                return True
            text = await response.text()
            raise RuntimeError(f"Failed to delete point: {text}")
    
    async def count(self, collection_name: str) -> int:
        """Count points in a collection."""
        session = self._get_session()
        async with session.post(
            f"{self.base_url}/collections/{collection_name}/points/count",
            headers=self._get_headers(),
            json={}
        ) as response:
            if response.status == 200:
                data = await response.json()
                return data["result"]["count"]
            text = await response.text()
            raise RuntimeError(f"Failed to count points: {text}")
