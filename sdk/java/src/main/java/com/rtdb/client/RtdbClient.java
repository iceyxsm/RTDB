package com.rtdb.client;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.github.benmanes.caffeine.cache.Cache;
import com.github.benmanes.caffeine.cache.Caffeine;
import io.github.resilience4j.circuitbreaker.CircuitBreaker;
import io.github.resilience4j.circuitbreaker.CircuitBreakerConfig;
import io.github.resilience4j.retry.Retry;
import io.github.resilience4j.retry.RetryConfig;
import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.Timer;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import okhttp3.*;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.time.Duration;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Supplier;

/**
 * Production-grade RTDB Java client with SIMDX optimizations, connection pooling,
 * circuit breaker, retry logic, and comprehensive metrics.
 */
public class RtdbClient implements AutoCloseable {
    private static final Logger logger = LoggerFactory.getLogger(RtdbClient.class);
    
    private final RtdbConfig config;
    private final OkHttpClient httpClient;
    private final ObjectMapper objectMapper;
    private final CircuitBreaker circuitBreaker;
    private final Retry retry;
    private final MeterRegistry meterRegistry;
    private final Cache<String, Object> queryCache;
    private final ExecutorService executorService;
    private final SIMDXOptimizer simdxOptimizer;
    
    // Metrics
    private final Timer searchTimer;
    private final Timer insertTimer;
    
    private volatile boolean closed = false;

    public RtdbClient(RtdbConfig config) {
        this.config = config;
        this.objectMapper = new ObjectMapper();
        this.meterRegistry = new SimpleMeterRegistry();
        this.simdxOptimizer = new SIMDXOptimizer(config.isEnableSIMDX());
        
        // Initialize HTTP client with connection pooling
        this.httpClient = new OkHttpClient.Builder()
                .connectionPool(new ConnectionPool(
                        config.getMaxConnections(),
                        config.getKeepAliveDuration().toMillis(),
                        TimeUnit.MILLISECONDS))
                .connectTimeout(config.getConnectionTimeout())
                .readTimeout(config.getRequestTimeout())
                .writeTimeout(config.getRequestTimeout())
                .retryOnConnectionFailure(true)
                .addInterceptor(new AuthenticationInterceptor(config.getApiKey()))
                .addInterceptor(new MetricsInterceptor(meterRegistry))
                .build();
        
        // Initialize circuit breaker
        CircuitBreakerConfig cbConfig = CircuitBreakerConfig.custom()
                .failureRateThreshold(config.getFailureThreshold())
                .waitDurationInOpenState(config.getRecoveryTimeout())
                .slidingWindowSize(100)
                .minimumNumberOfCalls(10)
                .build();
        this.circuitBreaker = CircuitBreaker.of("rtdb-client", cbConfig);
        
        // Initialize retry
        RetryConfig retryConfig = RetryConfig.custom()
                .maxAttempts(config.getMaxRetries())
                .waitDuration(config.getRetryBackoff())
                .exponentialBackoffMultiplier(config.getRetryMultiplier())
                .build();
        this.retry = Retry.of("rtdb-client", retryConfig);
        
        // Initialize cache
        this.queryCache = Caffeine.newBuilder()
                .maximumSize(config.getCacheSize())
                .expireAfterWrite(config.getCacheTtl())
                .recordStats()
                .build();
        
        // Initialize thread pool
        this.executorService = Executors.newFixedThreadPool(
                config.getThreadPoolSize(),
                r -> {
                    Thread t = new Thread(r, "rtdb-client-worker");
                    t.setDaemon(true);
                    return t;
                });
        
        // Initialize metrics
        this.searchTimer = Timer.builder("rtdb.search")
                .description("Search operation latency")
                .register(meterRegistry);
        this.insertTimer = Timer.builder("rtdb.insert")
                .description("Insert operation latency")
                .register(meterRegistry);
        
        logger.info("RTDB client initialized with SIMDX optimization: {}", config.isEnableSIMDX());
    }

    /**
     * Performs vector similarity search with SIMDX acceleration
     */
    public CompletableFuture<SearchResponse> searchAsync(SearchRequest request) {
        if (closed) {
            return CompletableFuture.failedFuture(new IllegalStateException("Client is closed"));
        }

        return CompletableFuture.supplyAsync(() -> {
            return searchTimer.recordCallable(() -> {
                // Apply SIMDX optimizations
                if (config.isEnableSIMDX() && request.isUseSIMDX()) {
                    request = simdxOptimizer.optimizeSearchRequest(request);
                }
                
                // Check cache first
                String cacheKey = generateCacheKey(request);
                SearchResponse cached = (SearchResponse) queryCache.getIfPresent(cacheKey);
                if (cached != null) {
                    logger.debug("Cache hit for search request");
                    return cached;
                }
                
                // Execute with circuit breaker and retry
                Supplier<SearchResponse> searchSupplier = () -> executeSearch(request);
                SearchResponse response = circuitBreaker.executeSupplier(
                        retry.decorate(searchSupplier));
                
                // Cache the result
                queryCache.put(cacheKey, response);
                
                return response;
            });
        }, executorService);
    }

    /**
     * Synchronous search method
     */
    public SearchResponse search(SearchRequest request) {
        try {
            return searchAsync(request).get(config.getRequestTimeout().toMillis(), TimeUnit.MILLISECONDS);
        } catch (InterruptedException | ExecutionException | TimeoutException e) {
            throw new RtdbException("Search operation failed", e);
        }
    }

    /**
     * Inserts vectors with batch optimization
     */
    public CompletableFuture<InsertResponse> insertAsync(String collectionName, List<Vector> vectors) {
        if (closed) {
            return CompletableFuture.failedFuture(new IllegalStateException("Client is closed"));
        }

        return CompletableFuture.supplyAsync(() -> {
            return insertTimer.recordCallable(() -> {
                // Apply SIMDX optimizations
                if (config.isEnableSIMDX()) {
                    vectors = simdxOptimizer.optimizeVectors(vectors);
                }
                
                // Process in batches for optimal performance
                List<CompletableFuture<InsertResponse>> batchFutures = new ArrayList<>();
                int batchSize = config.getBatchSize();
                
                for (int i = 0; i < vectors.size(); i += batchSize) {
                    int end = Math.min(i + batchSize, vectors.size());
                    List<Vector> batch = vectors.subList(i, end);
                    
                    CompletableFuture<InsertResponse> batchFuture = CompletableFuture.supplyAsync(() -> {
                        InsertRequest batchRequest = new InsertRequest(collectionName, batch);
                        return executeInsert(batchRequest);
                    }, executorService);
                    
                    batchFutures.add(batchFuture);
                }
                
                // Wait for all batches to complete
                CompletableFuture<Void> allBatches = CompletableFuture.allOf(
                        batchFutures.toArray(new CompletableFuture[0]));
                
                try {
                    allBatches.get();
                    return new InsertResponse(true, vectors.size(), "Successfully inserted all vectors");
                } catch (Exception e) {
                    throw new RtdbException("Batch insert failed", e);
                }
            });
        }, executorService);
    }

    /**
     * Creates a collection with SIMDX-optimized parameters
     */
    public CompletableFuture<CreateCollectionResponse> createCollectionAsync(CreateCollectionRequest request) {
        if (closed) {
            return CompletableFuture.failedFuture(new IllegalStateException("Client is closed"));
        }

        return CompletableFuture.supplyAsync(() -> {
            // Apply SIMDX-optimized collection parameters
            if (config.isEnableSIMDX()) {
                request = simdxOptimizer.optimizeCollectionConfig(request);
            }
            
            return executeCreateCollection(request);
        }, executorService);
    }

    /**
     * Executes the actual search operation
     */
    private SearchResponse executeSearch(SearchRequest request) {
        try {
            String url = String.format("%s/collections/%s/points/search", 
                    config.getBaseUrl(), request.getCollectionName());
            
            RequestBody body = RequestBody.create(
                    objectMapper.writeValueAsString(request),
                    MediaType.get("application/json"));
            
            Request httpRequest = new Request.Builder()
                    .url(url)
                    .post(body)
                    .build();
            
            try (Response response = httpClient.newCall(httpRequest).execute()) {
                if (!response.isSuccessful()) {
                    throw new RtdbException("Search failed: " + response.code() + " " + response.message());
                }
                
                String responseBody = response.body().string();
                return objectMapper.readValue(responseBody, SearchResponse.class);
            }
        } catch (IOException e) {
            throw new RtdbException("Search operation failed", e);
        }
    }

    /**
     * Executes the actual insert operation
     */
    private InsertResponse executeInsert(InsertRequest request) {
        try {
            String url = String.format("%s/collections/%s/points", 
                    config.getBaseUrl(), request.getCollectionName());
            
            RequestBody body = RequestBody.create(
                    objectMapper.writeValueAsString(request),
                    MediaType.get("application/json"));
            
            Request httpRequest = new Request.Builder()
                    .url(url)
                    .put(body)
                    .build();
            
            try (Response response = httpClient.newCall(httpRequest).execute()) {
                if (!response.isSuccessful()) {
                    throw new RtdbException("Insert failed: " + response.code() + " " + response.message());
                }
                
                String responseBody = response.body().string();
                return objectMapper.readValue(responseBody, InsertResponse.class);
            }
        } catch (IOException e) {
            throw new RtdbException("Insert operation failed", e);
        }
    }

    /**
     * Executes collection creation
     */
    private CreateCollectionResponse executeCreateCollection(CreateCollectionRequest request) {
        try {
            String url = String.format("%s/collections/%s", config.getBaseUrl(), request.getName());
            
            RequestBody body = RequestBody.create(
                    objectMapper.writeValueAsString(request),
                    MediaType.get("application/json"));
            
            Request httpRequest = new Request.Builder()
                    .url(url)
                    .put(body)
                    .build();
            
            try (Response response = httpClient.newCall(httpRequest).execute()) {
                if (!response.isSuccessful()) {
                    throw new RtdbException("Collection creation failed: " + response.code() + " " + response.message());
                }
                
                return new CreateCollectionResponse(true, "Collection created successfully");
            }
        } catch (IOException e) {
            throw new RtdbException("Collection creation failed", e);
        }
    }

    /**
     * Generates cache key for search requests
     */
    private String generateCacheKey(SearchRequest request) {
        try {
            return "search:" + objectMapper.writeValueAsString(request).hashCode();
        } catch (Exception e) {
            return "search:" + request.hashCode();
        }
    }

    /**
     * Gets client metrics
     */
    public ClientMetrics getMetrics() {
        return new ClientMetrics(
                meterRegistry,
                circuitBreaker.getMetrics(),
                queryCache.stats()
        );
    }

    /**
     * Health check
     */
    public CompletableFuture<Boolean> healthCheck() {
        if (closed) {
            return CompletableFuture.completedFuture(false);
        }

        return CompletableFuture.supplyAsync(() -> {
            try {
                Request request = new Request.Builder()
                        .url(config.getBaseUrl() + "/health")
                        .get()
                        .build();
                
                try (Response response = httpClient.newCall(request).execute()) {
                    return response.isSuccessful();
                }
            } catch (Exception e) {
                logger.warn("Health check failed", e);
                return false;
            }
        }, executorService);
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }
        
        closed = true;
        
        try {
            executorService.shutdown();
            if (!executorService.awaitTermination(5, TimeUnit.SECONDS)) {
                executorService.shutdownNow();
            }
        } catch (InterruptedException e) {
            executorService.shutdownNow();
            Thread.currentThread().interrupt();
        }
        
        httpClient.dispatcher().executorService().shutdown();
        httpClient.connectionPool().evictAll();
        
        logger.info("RTDB client closed");
    }
}