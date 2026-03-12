//! gRPC API implementation
#![cfg(feature = "grpc")]

use crate::collection::CollectionManager;
use crate::{CollectionConfig, Distance as RTDBDistance, SearchRequest, UpsertRequest, Vector, WithPayload};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Use pre-generated proto code
pub mod proto {
    include!("generated/rtdb.rs");
}

use proto::{
    CreateCollectionRequest, DeleteCollectionRequest, DeletePointsRequest,
    GetCollectionRequest, GetCollectionResponse, GetPointsRequest, GetPointsResponse,
    ListCollectionsRequest, ListCollectionsResponse, RetrievedPoint,
    ScoredPoint, SearchPointsRequest, SearchPointsResponse, UpsertPointsRequest,
    CollectionOperationResponse, PointsOperationResponse, PointsOperationResponseBody,
    CollectionInfo, CollectionConfig as ProtoCollectionConfig, VectorParams,
    CollectionDescription, Distance, CollectionOperationResponseBody,
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

        let config = CollectionConfig {
            dimension,
            distance,
            hnsw_config: None,
            quantization_config: None,
            optimizer_config: None,
        };

        match self.collections.create_collection(&req.collection_name, config) {
            Ok(_) => Ok(Response::new(CollectionOperationResponse {
                result: Some(CollectionOperationResponseBody {
                    success: true,
                    message: format!("Collection '{}' created", req.collection_name),
                }),
                time: 0.0,
            })),
            Err(e) => Ok(Response::new(CollectionOperationResponse {
                result: Some(CollectionOperationResponseBody {
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
                result: Some(CollectionOperationResponseBody {
                    success: true,
                    message: format!("Collection '{}' deleted", req.collection_name),
                }),
                time: 0.0,
            })),
            Err(e) => Ok(Response::new(CollectionOperationResponse {
                result: Some(CollectionOperationResponseBody {
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
                let dimension = 128;
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

// Manually implement server modules since we don't have tonic-build generated code
pub mod collections_server {
    use super::*;
    use tonic::codegen::*;
    use tonic::body::BoxBody;
    
    #[tonic::async_trait]
    pub trait Collections: Send + Sync + 'static {
        async fn list(
            &self,
            request: Request<super::ListCollectionsRequest>,
        ) -> Result<Response<super::ListCollectionsResponse>, Status>;

        async fn create(
            &self,
            request: Request<super::CreateCollectionRequest>,
        ) -> Result<Response<super::CollectionOperationResponse>, Status>;

        async fn delete(
            &self,
            request: Request<super::DeleteCollectionRequest>,
        ) -> Result<Response<super::CollectionOperationResponse>, Status>;

        async fn get(
            &self,
            request: Request<super::GetCollectionRequest>,
        ) -> Result<Response<super::GetCollectionResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct CollectionsServer<T: Collections> {
        inner: Arc<T>,
    }

    impl<T: Collections> CollectionsServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: Arc::new(inner),
            }
        }
    }

    impl<T: Collections> Service<http::Request<BoxBody>> for CollectionsServer<T> {
        type Response = http::Response<BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<BoxBody>) -> Self::Future {
            let inner = self.inner.clone();
            
            match req.uri().path() {
                "/rtdb.Collections/List" => {
                    struct ListSvc<T: Collections>(pub Arc<T>);
                    impl<T: Collections> tonic::server::UnaryService<super::ListCollectionsRequest> for ListSvc<T> {
                        type Response = super::ListCollectionsResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::ListCollectionsRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.list(request).await })
                        }
                    }
                    let fut = async move {
                        let method = ListSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Collections/Create" => {
                    struct CreateSvc<T: Collections>(pub Arc<T>);
                    impl<T: Collections> tonic::server::UnaryService<super::CreateCollectionRequest> for CreateSvc<T> {
                        type Response = super::CollectionOperationResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::CreateCollectionRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.create(request).await })
                        }
                    }
                    let fut = async move {
                        let method = CreateSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Collections/Delete" => {
                    struct DeleteSvc<T: Collections>(pub Arc<T>);
                    impl<T: Collections> tonic::server::UnaryService<super::DeleteCollectionRequest> for DeleteSvc<T> {
                        type Response = super::CollectionOperationResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::DeleteCollectionRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.delete(request).await })
                        }
                    }
                    let fut = async move {
                        let method = DeleteSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Collections/Get" => {
                    struct GetSvc<T: Collections>(pub Arc<T>);
                    impl<T: Collections> tonic::server::UnaryService<super::GetCollectionRequest> for GetSvc<T> {
                        type Response = super::GetCollectionResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::GetCollectionRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.get(request).await })
                        }
                    }
                    let fut = async move {
                        let method = GetSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
        }
    }
}

// Implement NamedService manually
impl<T: collections_server::Collections> tonic::server::NamedService for collections_server::CollectionsServer<T> {
    const NAME: &'static str = "rtdb.Collections";
}

pub mod points_server {
    use super::*;
    use tonic::codegen::*;
    use tonic::body::BoxBody;
    
    #[tonic::async_trait]
    pub trait Points: Send + Sync + 'static {
        async fn upsert(
            &self,
            request: Request<super::UpsertPointsRequest>,
        ) -> Result<Response<super::PointsOperationResponse>, Status>;

        async fn delete(
            &self,
            request: Request<super::DeletePointsRequest>,
        ) -> Result<Response<super::PointsOperationResponse>, Status>;

        async fn get(
            &self,
            request: Request<super::GetPointsRequest>,
        ) -> Result<Response<super::GetPointsResponse>, Status>;

        async fn search(
            &self,
            request: Request<super::SearchPointsRequest>,
        ) -> Result<Response<super::SearchPointsResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct PointsServer<T: Points> {
        inner: Arc<T>,
    }

    impl<T: Points> PointsServer<T> {
        pub fn new(inner: T) -> Self {
            Self {
                inner: Arc::new(inner),
            }
        }
    }

    impl<T: Points> Service<http::Request<BoxBody>> for PointsServer<T> {
        type Response = http::Response<BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<BoxBody>) -> Self::Future {
            let inner = self.inner.clone();
            
            match req.uri().path() {
                "/rtdb.Points/Upsert" => {
                    struct UpsertSvc<T: Points>(pub Arc<T>);
                    impl<T: Points> tonic::server::UnaryService<super::UpsertPointsRequest> for UpsertSvc<T> {
                        type Response = super::PointsOperationResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::UpsertPointsRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.upsert(request).await })
                        }
                    }
                    let fut = async move {
                        let method = UpsertSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Points/Delete" => {
                    struct DeleteSvc<T: Points>(pub Arc<T>);
                    impl<T: Points> tonic::server::UnaryService<super::DeletePointsRequest> for DeleteSvc<T> {
                        type Response = super::PointsOperationResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::DeletePointsRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.delete(request).await })
                        }
                    }
                    let fut = async move {
                        let method = DeleteSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Points/Get" => {
                    struct GetSvc<T: Points>(pub Arc<T>);
                    impl<T: Points> tonic::server::UnaryService<super::GetPointsRequest> for GetSvc<T> {
                        type Response = super::GetPointsResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::GetPointsRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.get(request).await })
                        }
                    }
                    let fut = async move {
                        let method = GetSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/rtdb.Points/Search" => {
                    struct SearchSvc<T: Points>(pub Arc<T>);
                    impl<T: Points> tonic::server::UnaryService<super::SearchPointsRequest> for SearchSvc<T> {
                        type Response = super::SearchPointsResponse;
                        type Future = BoxFuture<Response<Self::Response>, Status>;
                        fn call(&mut self, request: Request<super::SearchPointsRequest>) -> Self::Future {
                            let inner = self.0.clone();
                            Box::pin(async move { inner.search(request).await })
                        }
                    }
                    let fut = async move {
                        let method = SearchSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
        }
    }
}


// Implement NamedService manually
impl<T: points_server::Points> tonic::server::NamedService for points_server::PointsServer<T> {
    const NAME: &'static str = "rtdb.Points";
}
