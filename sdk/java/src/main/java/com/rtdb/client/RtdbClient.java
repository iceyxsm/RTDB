package com.rtdb.client;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.datatype.jsr310.JavaTimeModule;
import io.github.resilience4j.circuitbreaker.CircuitBreaker;
import io.github.resilience4j.circuitbreaker.CircuitBreakerConfig;
import io.github.resilience4j.ratelimiter.RateLimiter;
import io.github.resilience4j.ratelimiter.RateLimiterConfig;
import io.github.resilience4j.retry.Retry;
import io.github.resilience4j.retry.RetryConfig;
import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.Timer;
import okhttp3.*;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;
import java.time.Duration;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.function.Supplier;

/**
 * Production-grade Java client for RTDB vector database.
 * 
 * Features:
 * - High performance with connection pooling
 * - Built-in resilience (circuit breaker, retry, rate limiting)
 * - Comprehensive metrics and observability
 * - Async/sync API support
 * - Type-safe operations
 * 
 * Example usage:
 * <pre>
 * RTDBConfig config = RTDBConfig.builder()
 *     .endpoint("http://localhost:8080")
 *     .timeout(Duration.ofSeconds(30))
 *     .build();
 * 
 * RTDBClient client = new RTDBClient(config);
 * 
 * // Create collection
 * Collection collection = client.createCollection("my_vectors", 768).get();
 * 
 * // Insert vectors
 * List&lt;Vector&gt; vectors = Arrays.asList(
 *     new Vector("doc1", Arrays.asList(0.1f, 0.2f, 0.3f)),
 *     new Vector("doc2", Arrays.asList(0.4f, 0.5f, 0.6f))
 * );
 * client.insertVectors("my_vectors", vectors).get();
 * 
 * // Search
 * SearchResponse results = client.search("my_vectors", 
 *     Arrays.asList(0.1f, 0.2f, 0.3f), 10).get();
 * </pre>
 */
public class RTDBClient implements AutoCloseable {
    
    private static final Logger logger = LoggerFactory.getLogger(RTDBClient.class);
    private static final MediaType JSON = MediaType.get("application/json; charset=utf-8");
    
    private final RTDBConfig config;
    private final OkHttpClient httpClient;
    private final ObjectMapper objectMapper;
    private final CircuitBreaker circuitBreaker;
    private final RateLimiter rateLimiter;
    private final Retry retry;
    private final MeterRegistry meterRegistry;
    private final Timer requestTimer;
    
    /**
     * Creates a new RTDB client with the specified configuration.
     * 
     * @param config the client configuration
     * @throws IllegalArgumentException if config is null or invalid
     */
    public RTDBClient(RTDBConfig config) {
        if (config == null) {
            throw new IllegalArgumentException("Config cannot be null");
        }
        
        this.config = config;
        this.meterRegistry = config.getMeterRegistry();
        
        // Initialize HTTP client with connection pooling
        ConnectionPool connectionPool = new ConnectionPool(
            config.getMaxIdleConnections(),
            config.getKeepAliveDuration().toMillis(),
            TimeUnit.MILLISECONDS
        );
        
        this.httpClient = new OkHttpClient.Builder()
            .connectTimeout(config.getConnectTimeout())
            .readTimeout(config.getTimeout())
            .writeTimeout(config.getTimeout())
            .connectionPool(connectionPool)
            .addInterceptor(new MetricsInterceptor(meterRegistry))
            .addInterceptor(new AuthInterceptor(config.getApiKey()))
            .build();
        
        // Initialize JSON mapper
        this.objectMapper = new ObjectMapper()
            .registerModule(new JavaTimeModule());
        
        // Initialize resilience components
        this.circuitBreaker = CircuitBreaker.of("rtdb-client", 
            CircuitBreakerConfig.custom()
                .failureRateThreshold(config.getCircuitBreakerFailureThreshold())
                .waitDurationInOpenState(config.getCircuitBreakerWaitDuration())
                .slidingWindowSize(config.getCircuitBreakerSlidingWindowSize())
                .minimumNumberOfCalls(config.getCircuitBreakerMinimumCalls())
                .build());
        
        this.rateLimiter = RateLimiter.of("rtdb-client",
            RateLimiterConfig.custom()
                .limitForPeriod(config.getRateLimitPermits())
                .limitRefreshPeriod(config.getRateLimitPeriod())
                .timeoutDuration(config.getRateLimitTimeout())
                .build());
        
        this.retry = Retry.of("rtdb-client",
            RetryConfig.custom()
                .maxAttempts(config.getRetryMaxAttempts())
                .waitDuration(config.getRetryWaitDuration())
                .retryExceptions(IOException.class, RTDBException.class)
                .build());
        
        this.requestTimer = Timer.builder("rtdb.client.requests")
            .description("RTDB client request latency")
            .register(meterRegistry);
        
        logger.info("RTDB client initialized with endpoint: {}", config.getEndpoint());
        
        // Perform initial health check
        try {
            healthCheck().get();
            logger.info("Initial health check successful");
        } catch (Exception e) {
            logger.warn("Initial health check failed: {}", e.getMessage());
        }
    }