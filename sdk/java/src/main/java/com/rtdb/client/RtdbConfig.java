package com.rtdb.client;

import java.time.Duration;

/**
 * Configuration class for RTDB client with production-grade defaults
 */
public class RtdbConfig {
    private String baseUrl = "http://localhost:6333";
    private String apiKey;
    private Duration connectionTimeout = Duration.ofSeconds(30);
    private Duration requestTimeout = Duration.ofSeconds(10);
    private Duration keepAliveDuration = Duration.ofMinutes(5);
    private int maxConnections = 10;
    private int maxRetries = 3;
    private Duration retryBackoff = Duration.ofMillis(100);
    private double retryMultiplier = 2.0;
    private float failureThreshold = 50.0f; // 50% failure rate
    private Duration recoveryTimeout = Duration.ofSeconds(30);
    private int batchSize = 1000;
    private int threadPoolSize = Runtime.getRuntime().availableProcessors();
    private boolean enableSIMDX = true;
    private long cacheSize = 10000;
    private Duration cacheTtl = Duration.ofMinutes(10);
    
    // Builder pattern for easy configuration
    public static class Builder {
        private final RtdbConfig config = new RtdbConfig();
        
        public Builder baseUrl(String baseUrl) {
            config.baseUrl = baseUrl;
            return this;
        }
        
        public Builder apiKey(String apiKey) {
            config.apiKey = apiKey;
            return this;
        }
        
        public Builder connectionTimeout(Duration timeout) {
            config.connectionTimeout = timeout;
            return this;
        }
        
        public Builder requestTimeout(Duration timeout) {
            config.requestTimeout = timeout;
            return this;
        }
        
        public Builder maxConnections(int maxConnections) {
            config.maxConnections = maxConnections;
            return this;
        }
        
        public Builder maxRetries(int maxRetries) {
            config.maxRetries = maxRetries;
            return this;
        }
        
        public Builder retryBackoff(Duration backoff) {
            config.retryBackoff = backoff;
            return this;
        }
        
        public Builder retryMultiplier(double multiplier) {
            config.retryMultiplier = multiplier;
            return this;
        }
        
        public Builder failureThreshold(float threshold) {
            config.failureThreshold = threshold;
            return this;
        }
        
        public Builder recoveryTimeout(Duration timeout) {
            config.recoveryTimeout = timeout;
            return this;
        }
        
        public Builder batchSize(int batchSize) {
            config.batchSize = batchSize;
            return this;
        }
        
        public Builder threadPoolSize(int size) {
            config.threadPoolSize = size;
            return this;
        }
        
        public Builder enableSIMDX(boolean enable) {
            config.enableSIMDX = enable;
            return this;
        }
        
        public Builder cacheSize(long size) {
            config.cacheSize = size;
            return this;
        }
        
        public Builder cacheTtl(Duration ttl) {
            config.cacheTtl = ttl;
            return this;
        }
        
        public RtdbConfig build() {
            return config;
        }
    }
    
    public static Builder builder() {
        return new Builder();
    }
    
    // Getters
    public String getBaseUrl() { return baseUrl; }
    public String getApiKey() { return apiKey; }
    public Duration getConnectionTimeout() { return connectionTimeout; }
    public Duration getRequestTimeout() { return requestTimeout; }
    public Duration getKeepAliveDuration() { return keepAliveDuration; }
    public int getMaxConnections() { return maxConnections; }
    public int getMaxRetries() { return maxRetries; }
    public Duration getRetryBackoff() { return retryBackoff; }
    public double getRetryMultiplier() { return retryMultiplier; }
    public float getFailureThreshold() { return failureThreshold; }
    public Duration getRecoveryTimeout() { return recoveryTimeout; }
    public int getBatchSize() { return batchSize; }
    public int getThreadPoolSize() { return threadPoolSize; }
    public boolean isEnableSIMDX() { return enableSIMDX; }
    public long getCacheSize() { return cacheSize; }
    public Duration getCacheTtl() { return cacheTtl; }
}