//! gRPC API implementation

use crate::collection::CollectionManager;
use crate::{CollectionConfig, Distance as RTDBDistance, SearchRequest, UpsertRequest, Vector};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Include generated proto code
pub mod proto {
    tonic::include_proto!("rtdb");
}

use proto::{
    collections_server::{Collections, CollectionsServer},
    points_server::{Points, PointsServer},
    CreateCollectionRequest, DeleteCollectionRequest, DeletePointsRequest,
    GetCollectionRequest, GetCollectionResponse, GetPointsRequest, GetPointsResponse,
    ListCollectionsRequest, ListCollectionsResponse, PointStruct, RetrievedPoint,
    ScoredPoint, SearchPointsRequest, SearchPointsResponse, UpsertPointsRequest,
    CollectionOperationResponse, PointsOperationResponse, PointsOperationResponseBody,
    CollectionInfo, CollectionConfig as ProtoCollectionConfig, VectorParams,
    CollectionDescription,
};

/// gRPC Collections service
pub struct CollectionsService {
    collections: Arc<CollectionManager>,
}

impl CollectionsService {
    /// Create new service
    pub fn new(collections: Arc<CollectionManager>) -> Self {
        Self { collections }
    }

    /// Get server
    pub fn into_server(self) -> CollectionsServer<Self> {
        CollectionsServer::new(self)
    }
}

#[tonic::async_trait]
impl Collections for CollectionsService {
    async fn list(
        &self,
        _request: Request<ListCollectionsRequest>,
    ) -> Result<Response<ListCollectionsResponse>, Status> {
        let names = self.collections.list_collections();
        let descriptions: Vec<_> = names
            .into_iter()
            .map(|name| CollectionDescription { name })
            .collect();

        Ok(Response::new(ListCollectionsResponse {
            collections: descriptions,
            time: 0.0,
        }))
    }

    async fn create(
        &self,
        request: Request<CreateCollectionRequest>,
    ) -> Result<Response<CollectionOperationResponse>, Status> {
        let req = request.into_inner();
        
        let distance = match req.vectors_config.as_ref().map(|c| c.distance) {
            Some(1) => RTDBDistance::Cosine,
            Some(2) => RTDBDistance::Euclidean,
            Some(3) => RTDBDistance::Dot,
            _ => RTDBDistance::Cosine,
        };

        let config = CollectionConfig {
            dimension: req.vectors_config.map(|c| c.size as usize).unwrap_or(128),
            distance,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };

        match self.collections.create_collection(&req.collection_name, config) {
            Ok(_) => Ok(Response::new(CollectionOperationResponse {
                result: true,
                time: 0.0,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteCollectionRequest>,
    ) -> Result<Response<CollectionOperationResponse>, Status> {
        let req = request.into_inner();
        
        match self.collections.delete_collection(&req.collection_name) {
            Ok(_) => Ok(Response::new(CollectionOperationResponse {
                result: true,
                time: 0.0,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn get(
        &self,
        request: Request<GetCollectionRequest>,
    ) -> Result<Response<GetCollectionResponse>, Status> {
        let req = request.into_inner();
        
        match self.collections.get_collection(&req.collection_name) {
            Ok(collection) => {
                let config = collection.config();
                let distance = match config.distance {
                    RTDBDistance::Cosine => 1,
                    RTDBDistance::Euclidean => 2,
                    RTDBDistance::Dot => 3,
                    _ => 0,
                };

                let info = CollectionInfo {
                    name: collection.name().to_string(),
                    vectors_count: collection.vector_count(),
                    config: Some(ProtoCollectionConfig {
                        params: Some(VectorParams {
                            size: config.dimension as u64,
                            distance,
                        }),
                    }),
                };

                Ok(Response::new(GetCollectionResponse {
                    result: Some(info),
                    time: 0.0,
                }))
            }
            Err(e) => Err(Status::not_found(e.to_string())),
        }
    }
}

/// gRPC Points service
pub struct PointsService {
    collections: Arc<CollectionManager>,
}

impl PointsService {
    /// Create new service
    pub fn new(collections: Arc<CollectionManager>) -> Self {
        Self { collections }
    }

    /// Get server
    pub fn into_server(self) -> PointsServer<Self> {
        PointsServer::new(self)
    }
}

#[tonic::async_trait]
impl Points for PointsService {
    async fn upsert(
        &self,
        request: Request<UpsertPointsRequest>,
    ) -> Result<Response<PointsOperationResponse>, Status> {
        let req = request.into_inner();
        
        let collection = self.collections
            .get_collection(&req.collection_name)
            .map_err(|e| Status::not_found(e.to_string()))?;

        let vectors: Vec<(u64, Vector)> = req
            .points
            .into_iter()
            .map(|p| (p.id, Vector::new(p.vector)))
            .collect();

        let upsert_request = UpsertRequest { vectors };

        match collection.upsert(upsert_request) {
            Ok(info) => Ok(Response::new(PointsOperationResponse {
                result: Some(PointsOperationResponseBody {
                    operation_id: info.operation_id,
                    status: "completed".to_string(),
                }),
                time: 0.0,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn delete(
        &self,
        request: Request<DeletePointsRequest>,
    ) -> Result<Response<PointsOperationResponse>, Status> {
        let req = request.into_inner();
        
        let collection = self.collections
            .get_collection(&req.collection_name)
            .map_err(|e| Status::not_found(e.to_string()))?;

        match collection.delete(&req.ids) {
            Ok(_) => Ok(Response::new(PointsOperationResponse {
                result: Some(PointsOperationResponseBody {
                    operation_id: 0,
                    status: "completed".to_string(),
                }),
                time: 0.0,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn get(
        &self,
        request: Request<GetPointsRequest>,
    ) -> Result<Response<GetPointsResponse>, Status> {
        let req = request.into_inner();
        
        let collection = self.collections
            .get_collection(&req.collection_name)
            .map_err(|e| Status::not_found(e.to_string()))?;

        let mut points = Vec::new();
        for id in req.ids {
            match collection.get(id) {
                Ok(Some(vector)) => {
                    points.push(RetrievedPoint {
                        id,
                        vector: vector.vector,
                    });
                }
                _ => {}
            }
        }

        Ok(Response::new(GetPointsResponse {
            result: points,
            time: 0.0,
        }))
    }

    async fn search(
        &self,
        request: Request<SearchPointsRequest>,
    ) -> Result<Response<SearchPointsResponse>, Status> {
        let req = request.into_inner();
        
        let collection = self.collections
            .get_collection(&req.collection_name)
            .map_err(|e| Status::not_found(e.to_string()))?;

        let search_request = SearchRequest {
            vector: req.vector,
            limit: req.limit as usize,
            offset: 0,
            score_threshold: None,
            with_payload: None,
            with_vector: req.with_vectors,
            filter: None,
            params: None,
        };

        match collection.search(search_request) {
            Ok(results) => {
                let scored: Vec<_> = results
                    .into_iter()
                    .map(|r| ScoredPoint {
                        id: r.id,
                        score: r.score,
                        vector: r.vector.unwrap_or_default(),
                    })
                    .collect();

                Ok(Response::new(SearchPointsResponse {
                    result: scored,
                    time: 0.0,
                }))
            }
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
}

/// Start gRPC server
pub async fn start_server(
    port: u16,
    collections: Arc<CollectionManager>,
) -> crate::Result<()> {
    let addr = format!("[::1]:{}", port).parse()
        .map_err(|e| crate::RTDBError::Io(format!("{:?}", e)))?;

    let collections_service = CollectionsService::new(collections.clone());
    let points_service = PointsService::new(collections);

    tonic::transport::Server::builder()
        .add_service(collections_service.into_server())
        .add_service(points_service.into_server())
        .serve(addr)
        .await
        .map_err(|e| crate::RTDBError::Io(e.to_string()))?;

    Ok(())
}
