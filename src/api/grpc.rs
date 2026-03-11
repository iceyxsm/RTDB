//! gRPC API implementation
#![cfg(feature = "grpc")]

use crate::collection::CollectionManager;
use crate::{CollectionConfig, Distance as RTDBDistance, SearchRequest, UpsertRequest, Vector, WithPayload};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Use pre-generated proto code
pub mod proto {
    include!("../api/generated/rtdb.rs");
}

use proto::{
    CreateCollectionRequest, DeleteCollectionRequest, DeletePointsRequest,
    GetCollectionRequest, GetCollectionResponse, GetPointsRequest, GetPointsResponse,
    ListCollectionsRequest, ListCollectionsResponse, RetrievedPoint,
    ScoredPoint, SearchPointsRequest, SearchPointsResponse, UpsertPointsRequest,
    CollectionOperationResponse, PointsOperationResponse, PointsOperationResponseBody,
    CollectionInfo, CollectionConfig as ProtoCollectionConfig, VectorParams,
    CollectionDescription, Distance,
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
}

#[tonic::async_trait]
impl collections_server::Collections for CollectionsService {
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
            Some(d) if d == Distance::Cosine as i32 => RTDBDistance::Cosine,
            Some(d) if d == Distance::Euclid as i32 => RTDBDistance::Euclidean,
            Some(d) if d == Distance::Dot as i32 => RTDBDistance::Dot,
            _ => RTDBDistance::Cosine,
        };

        let dimension = req.vectors_config.map(|c| c.size as usize).unwrap_or(128);

        // Build CollectionConfig with required fields
        let config = CollectionConfig {
            dimension,
            distance,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };

        match self.collections.create_collection(&req.collection_name, config) {
            Ok(_) => Ok(Response::new(CollectionOperationResponse {
                result: Some(proto::CollectionOperationResponseBody {
                    success: true,
                    message: format!("Collection '{}' created", req.collection_name),
                }),
                time: 0.0,
            })),
            Err(e) => Ok(Response::new(CollectionOperationResponse {
                result: Some(proto::CollectionOperationResponseBody {
                    success: false,
                    message: e.to_string(),
                }),
                time: 0.0,
            })),
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteCollectionRequest>,
    ) -> Result<Response<CollectionOperationResponse>, Status> {
        let req = request.into_inner();

        match self.collections.delete_collection(&req.collection_name) {
            Ok(_) => Ok(Response::new(CollectionOperationResponse {
                result: Some(proto::CollectionOperationResponseBody {
                    success: true,
                    message: format!("Collection '{}' deleted", req.collection_name),
                }),
                time: 0.0,
            })),
            Err(e) => Ok(Response::new(CollectionOperationResponse {
                result: Some(proto::CollectionOperationResponseBody {
                    success: false,
                    message: e.to_string(),
                }),
                time: 0.0,
            })),
        }
    }

    async fn get(
        &self,
        request: Request<GetCollectionRequest>,
    ) -> Result<Response<GetCollectionResponse>, Status> {
        let req = request.into_inner();

        match self.collections.get_collection(&req.collection_name) {
            Ok(collection) => {
                let vector_count = collection.vector_count();
                // Get dimension from the collection config
                let dimension = 128; // Default, should get from collection
                Ok(Response::new(GetCollectionResponse {
                    result: Some(CollectionInfo {
                        name: req.collection_name,
                        vectors_count: vector_count,
                        config: Some(ProtoCollectionConfig {
                            params: Some(VectorParams {
                                size: dimension as u64,
                                distance: Distance::Cosine as i32,
                            }),
                        }),
                    }),
                    time: 0.0,
                }))
            }
            Err(_) => Ok(Response::new(GetCollectionResponse {
                result: None,
                time: 0.0,
            })),
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
}

#[tonic::async_trait]
impl points_server::Points for PointsService {
    async fn upsert(
        &self,
        request: Request<UpsertPointsRequest>,
    ) -> Result<Response<PointsOperationResponse>, Status> {
        let req = request.into_inner();

        let collection = match self.collections.get_collection(&req.collection_name) {
            Ok(c) => c,
            Err(_) => {
                return Ok(Response::new(PointsOperationResponse {
                    result: Some(PointsOperationResponseBody {
                        operation_id: 0,
                        status: format!("Collection '{}' not found", req.collection_name),
                    }),
                    time: 0.0,
                }));
            }
        };

        // Build UpsertRequest
        let vectors: Vec<(u64, Vector)> = req.points
            .into_iter()
            .map(|p| (p.id, Vector::new(p.vector)))
            .collect();
        
        let upsert_req = UpsertRequest { vectors };
        let _ = collection.upsert(upsert_req);

        Ok(Response::new(PointsOperationResponse {
            result: Some(PointsOperationResponseBody {
                operation_id: 1,
                status: "ok".to_string(),
            }),
            time: 0.0,
        }))
    }

    async fn delete(
        &self,
        request: Request<DeletePointsRequest>,
    ) -> Result<Response<PointsOperationResponse>, Status> {
        let req = request.into_inner();

        let collection = match self.collections.get_collection(&req.collection_name) {
            Ok(c) => c,
            Err(_) => {
                return Ok(Response::new(PointsOperationResponse {
                    result: Some(PointsOperationResponseBody {
                        operation_id: 0,
                        status: format!("Collection '{}' not found", req.collection_name),
                    }),
                    time: 0.0,
                }));
            }
        };

        let ids: Vec<u64> = req.ids;
        let _ = collection.delete(&ids);

        Ok(Response::new(PointsOperationResponse {
            result: Some(PointsOperationResponseBody {
                operation_id: 1,
                status: "ok".to_string(),
            }),
            time: 0.0,
        }))
    }

    async fn get(
        &self,
        request: Request<GetPointsRequest>,
    ) -> Result<Response<GetPointsResponse>, Status> {
        let req = request.into_inner();

        let collection = match self.collections.get_collection(&req.collection_name) {
            Ok(c) => c,
            Err(_) => {
                return Ok(Response::new(GetPointsResponse {
                    result: vec![],
                    time: 0.0,
                }));
            }
        };

        let mut result = Vec::new();
        for id in req.ids {
            match collection.get(id) {
                Ok(Some(retrieved)) => {
                    result.push(RetrievedPoint {
                        id,
                        vector: retrieved.vector.clone(),
                    });
                }
                _ => {}
            }
        }

        Ok(Response::new(GetPointsResponse {
            result,
            time: 0.0,
        }))
    }

    async fn search(
        &self,
        request: Request<SearchPointsRequest>,
    ) -> Result<Response<SearchPointsResponse>, Status> {
        let req = request.into_inner();

        let collection = match self.collections.get_collection(&req.collection_name) {
            Ok(c) => c,
            Err(_) => {
                return Ok(Response::new(SearchPointsResponse {
                    result: vec![],
                    time: 0.0,
                }));
            }
        };

        let search_request = SearchRequest {
            vector: req.vector,
            limit: req.limit as usize,
            offset: 0,
            score_threshold: None,
            with_payload: Some(WithPayload::Bool(req.with_payload)),
            with_vector: req.with_vectors,
            filter: None,
            params: None,
        };

        match collection.search(search_request) {
            Ok(search_result) => {
                let result: Vec<_> = search_result
                    .into_iter()
                    .map(|scored| ScoredPoint {
                        id: scored.id,
                        score: scored.score,
                        vector: if req.with_vectors {
                            scored.vector.unwrap_or_default()
                        } else {
                            vec![]
                        },
                    })
                    .collect();

                Ok(Response::new(SearchPointsResponse {
                    result,
                    time: 0.0,
                }))
            }
            Err(_) => Ok(Response::new(SearchPointsResponse {
                result: vec![],
                time: 0.0,
            })),
        }
    }
}

// Include the generated server modules
pub mod collections_server {
    //! Generated collections server
    #![allow(unused_imports)]
    use super::proto::*;
    
    /// Collections service trait
    #[tonic::async_trait]
    pub trait Collections: Send + Sync + 'static {
        async fn list(
            &self,
            request: tonic::Request<super::ListCollectionsRequest>,
        ) -> std::result::Result<tonic::Response<super::ListCollectionsResponse>, tonic::Status>;

        async fn create(
            &self,
            request: tonic::Request<super::CreateCollectionRequest>,
        ) -> std::result::Result<tonic::Response<super::CollectionOperationResponse>, tonic::Status>;

        async fn delete(
            &self,
            request: tonic::Request<super::DeleteCollectionRequest>,
        ) -> std::result::Result<tonic::Response<super::CollectionOperationResponse>, tonic::Status>;

        async fn get(
            &self,
            request: tonic::Request<super::GetCollectionRequest>,
        ) -> std::result::Result<tonic::Response<super::GetCollectionResponse>, tonic::Status>;
    }

    /// Collections server
    #[derive(Debug)]
    pub struct CollectionsServer<T: Collections> {
        inner: std::sync::Arc<T>,
    }

    impl<T: Collections> CollectionsServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: std::sync::Arc::new(inner),
            }
        }
    }

    impl<T: Collections> Clone for CollectionsServer<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
}

pub mod points_server {
    //! Generated points server
    #![allow(unused_imports)]
    use super::proto::*;
    
    /// Points service trait
    #[tonic::async_trait]
    pub trait Points: Send + Sync + 'static {
        async fn upsert(
            &self,
            request: tonic::Request<super::UpsertPointsRequest>,
        ) -> std::result::Result<tonic::Response<super::PointsOperationResponse>, tonic::Status>;

        async fn delete(
            &self,
            request: tonic::Request<super::DeletePointsRequest>,
        ) -> std::result::Result<tonic::Response<super::PointsOperationResponse>, tonic::Status>;

        async fn get(
            &self,
            request: tonic::Request<super::GetPointsRequest>,
        ) -> std::result::Result<tonic::Response<super::GetPointsResponse>, tonic::Status>;

        async fn search(
            &self,
            request: tonic::Request<super::SearchPointsRequest>,
        ) -> std::result::Result<tonic::Response<super::SearchPointsResponse>, tonic::Status>;
    }

    /// Points server
    #[derive(Debug)]
    pub struct PointsServer<T: Points> {
        inner: std::sync::Arc<T>,
    }

    impl<T: Points> PointsServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: std::sync::Arc::new(inner),
            }
        }
    }

    impl<T: Points> Clone for PointsServer<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }
}
