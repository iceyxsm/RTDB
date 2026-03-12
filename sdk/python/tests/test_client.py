"""Tests for the RTDB Python client."""

import asyncio
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

# Try to import the native extension first, fallback to pure Python
try:
    from rtdb_sdk.rtdb_python import RtdbClient, Point, SearchResult
    NATIVE_AVAILABLE = True
except ImportError:
    from rtdb_sdk.client import RtdbClient, Point, SearchResult
    NATIVE_AVAILABLE = False


@pytest.fixture
def client():
    """Create a test client."""
    return RtdbClient("http://localhost:6333", api_key="test-key")


@pytest.fixture
def mock_response():
    """Create a mock aiohttp response."""
    response = MagicMock()
    response.status = 200
    response.json = AsyncMock(return_value={"result": {"status": "ok"}})
    response.text = AsyncMock(return_value="error")
    return response


class TestRtdbClient:
    """Test cases for RtdbClient."""
    
    @pytest.mark.asyncio
    async def test_client_initialization(self):
        """Test client can be initialized."""
        client = RtdbClient("http://localhost:6333")
        assert client.base_url == "http://localhost:6333"
        assert client.api_key is None
        
        client_with_auth = RtdbClient("http://localhost:6333", api_key="secret")
        assert client_with_auth.api_key == "secret"
    
    @pytest.mark.asyncio
    async def test_is_healthy_success(self, client, mock_response):
        """Test health check returns True on success."""
        if not NATIVE_AVAILABLE:
            with patch("aiohttp.ClientSession.get") as mock_get:
                mock_get.return_value.__aenter__ = AsyncMock(return_value=mock_response)
                result = await client.is_healthy()
                assert result is True
    
    @pytest.mark.asyncio
    async def test_is_healthy_failure(self, client):
        """Test health check returns False on failure."""
        if not NATIVE_AVAILABLE:
            error_response = MagicMock()
            error_response.status = 503
            
            with patch("aiohttp.ClientSession.get") as mock_get:
                mock_get.return_value.__aenter__ = AsyncMock(return_value=error_response)
                result = await client.is_healthy()
                assert result is False
    
    @pytest.mark.asyncio
    async def test_create_collection_success(self, client, mock_response):
        """Test collection creation."""
        if not NATIVE_AVAILABLE:
            with patch("aiohttp.ClientSession.put") as mock_put:
                mock_put.return_value.__aenter__ = AsyncMock(return_value=mock_response)
                result = await client.create_collection("test_collection", 128)
                assert result is True
    
    @pytest.mark.asyncio
    async def test_list_collections(self, client):
        """Test listing collections."""
        if not NATIVE_AVAILABLE:
            response = MagicMock()
            response.status = 200
            response.json = AsyncMock(return_value={
                "result": {
                    "collections": [
                        {"name": "col1"},
                        {"name": "col2"}
                    ]
                }
            })
            
            with patch("aiohttp.ClientSession.get") as mock_get:
                mock_get.return_value.__aenter__ = AsyncMock(return_value=response)
                collections = await client.list_collections()
                assert collections == ["col1", "col2"]
    
    @pytest.mark.asyncio
    async def test_upsert_points(self, client, mock_response):
        """Test upserting points."""
        if not NATIVE_AVAILABLE:
            with patch("aiohttp.ClientSession.put") as mock_put:
                mock_put.return_value.__aenter__ = AsyncMock(return_value=mock_response)
                points = [{"id": 1, "vector": [0.1, 0.2, 0.3]}]
                result = await client.upsert("test_collection", points)
                assert result == "ok"
    
    @pytest.mark.asyncio
    async def test_search(self, client):
        """Test vector search."""
        if not NATIVE_AVAILABLE:
            response = MagicMock()
            response.status = 200
            response.json = AsyncMock(return_value={
                "result": [
                    {"id": 1, "score": 0.95, "payload": {"text": "hello"}},
                    {"id": 2, "score": 0.85, "payload": {"text": "world"}},
                ]
            })
            
            with patch("aiohttp.ClientSession.post") as mock_post:
                mock_post.return_value.__aenter__ = AsyncMock(return_value=response)
                results = await client.search("test_collection", [0.1, 0.2, 0.3], limit=10)
                
                assert len(results) == 2
                assert results[0].id == 1
                assert results[0].score == 0.95
                assert results[1].id == 2


class TestPoint:
    """Test cases for Point dataclass."""
    
    def test_point_creation(self):
        """Test Point can be created."""
        point = Point(id=1, vector=[0.1, 0.2, 0.3], payload={"key": "value"})
        assert point.id == 1
        assert point.vector == [0.1, 0.2, 0.3]
        assert point.payload == {"key": "value"}
    
    def test_point_without_payload(self):
        """Test Point can be created without payload."""
        point = Point(id=1, vector=[0.1, 0.2, 0.3])
        assert point.id == 1
        assert point.vector == [0.1, 0.2, 0.3]
        assert point.payload is None


class TestSearchResult:
    """Test cases for SearchResult dataclass."""
    
    def test_search_result_creation(self):
        """Test SearchResult can be created."""
        result = SearchResult(id=1, score=0.95, payload={"text": "hello"})
        assert result.id == 1
        assert result.score == 0.95
        assert result.payload == {"text": "hello"}
