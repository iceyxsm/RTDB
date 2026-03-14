// Package rtdb provides a production-grade Go client for RTDB vector database
package rtdb

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"time"

	"github.com/go-resty/resty/v2"
	"github.com/google/uuid"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sony/gobreaker"
	"go.uber.org/zap"
	"golang.org/x/time/rate"
)

// Client represents a high-performance RTDB client with production features
type Client struct {
	config         *Config
	httpClient     *resty.Client
	circuitBreaker *gobreaker.CircuitBreaker
	rateLimiter    *rate.Limiter
	metrics        *Metrics
	logger         *zap.Logger
	mu             sync.RWMutex
}

// Config holds the client configuration
type Config struct {
	Endpoint           string        `json:"endpoint"`
	Timeout            time.Duration `json:"timeout"`
	RetryCount         int           `json:"retry_count"`
	RetryWaitTime      time.Duration `json:"retry_wait_time"`
	MaxRetryWaitTime   time.Duration `json:"max_retry_wait_time"`
	RateLimitRPS       float64       `json:"rate_limit_rps"`
	RateLimitBurst     int           `json:"rate_limit_burst"`
	MaxIdleConns       int           `json:"max_idle_conns"`
	MaxConnsPerHost    int           `json:"max_conns_per_host"`
	IdleConnTimeout    time.Duration `json:"idle_conn_timeout"`
	CircuitBreakerName string        `json:"circuit_breaker_name"`
	UserAgent          string        `json:"user_agent"`
	APIKey             string        `json:"api_key"`
	BatchSize          int           `json:"batch_size"`
}

// DefaultConfig returns a production-ready default configuration
func DefaultConfig(endpoint string) *Config {
	return &Config{
		Endpoint:           endpoint,
		Timeout:            30 * time.Second,
		RetryCount:         3,
		RetryWaitTime:      1 * time.Second,
		MaxRetryWaitTime:   10 * time.Second,
		RateLimitRPS:       1000.0, // 1K RPS default
		RateLimitBurst:     100,
		MaxIdleConns:       100,
		MaxConnsPerHost:    10,
		IdleConnTimeout:    90 * time.Second,
		CircuitBreakerName: "rtdb-client",
		UserAgent:          fmt.Sprintf("rtdb-go-client/1.0.0"),
		BatchSize:          100,
	}
}

// NewClient creates a new RTDB client with the given configuration
func NewClient(config *Config) (*Client, error) {
	if config == nil {
		return nil, fmt.Errorf("config cannot be nil")
	}

	// Initialize logger
	logger, err := zap.NewProduction()
	if err != nil {
		return nil, fmt.Errorf("failed to initialize logger: %w", err)
	}

	// Initialize metrics
	metrics := NewMetrics()

	// Initialize rate limiter
	rateLimiter := rate.NewLimiter(rate.Limit(config.RateLimitRPS), config.RateLimitBurst)

	// Initialize circuit breaker
	cbSettings := gobreaker.Settings{
		Name:        config.CircuitBreakerName,
		MaxRequests: 3,
		Interval:    10 * time.Second,
		Timeout:     30 * time.Second,
		ReadyToTrip: func(counts gobreaker.Counts) bool {
			failureRatio := float64(counts.TotalFailures) / float64(counts.Requests)
			return counts.Requests >= 3 && failureRatio >= 0.6
		},
		OnStateChange: func(name string, from gobreaker.State, to gobreaker.State) {
			logger.Info("Circuit breaker state changed",
				zap.String("name", name),
				zap.String("from", from.String()),
				zap.String("to", to.String()))
		},
	}
	circuitBreaker := gobreaker.NewCircuitBreaker(cbSettings)

	// Initialize HTTP client
	httpClient := resty.New().
		SetTimeout(config.Timeout).
		SetRetryCount(config.RetryCount).
		SetRetryWaitTime(config.RetryWaitTime).
		SetRetryMaxWaitTime(config.MaxRetryWaitTime).
		SetHeader("User-Agent", config.UserAgent).
		SetHeader("Content-Type", "application/json").
		OnBeforeRequest(func(c *resty.Client, req *resty.Request) error {
			// Rate limiting
			if err := rateLimiter.Wait(context.Background()); err != nil {
				return fmt.Errorf("rate limit exceeded: %w", err)
			}
			
			// Add API key if configured
			if config.APIKey != "" {
				req.SetHeader("Authorization", "Bearer "+config.APIKey)
			}
			
			return nil
		}).
		OnAfterResponse(func(c *resty.Client, resp *resty.Response) error {
			// Record metrics
			metrics.RecordRequest(resp.Request.Method, resp.StatusCode(), resp.Time())
			return nil
		})

	// Configure HTTP transport
	httpClient.GetClient().Transport = &http.Transport{
		MaxIdleConns:        config.MaxIdleConns,
		MaxIdleConnsPerHost: config.MaxConnsPerHost,
		IdleConnTimeout:     config.IdleConnTimeout,
	}

	client := &Client{
		config:         config,
		httpClient:     httpClient,
		circuitBreaker: circuitBreaker,
		rateLimiter:    rateLimiter,
		metrics:        metrics,
		logger:         logger,
	}

	// Perform health check
	if err := client.HealthCheck(context.Background()); err != nil {
		logger.Warn("Initial health check failed", zap.Error(err))
	}

	logger.Info("RTDB client initialized successfully",
		zap.String("endpoint", config.Endpoint))

	return client, nil
}
// HealthCheck performs a health check against the RTDB server
func (c *Client) HealthCheck(ctx context.Context) error {
	start := time.Now()
	
	result, err := c.circuitBreaker.Execute(func() (interface{}, error) {
		resp, err := c.httpClient.R().
			SetContext(ctx).
			Get(c.config.Endpoint + "/health")
		
		if err != nil {
			return nil, err
		}
		
		if resp.StatusCode() != http.StatusOK {
			return nil, fmt.Errorf("health check failed with status: %d", resp.StatusCode())
		}
		
		return resp, nil
	})
	
	latency := time.Since(start)
	c.metrics.RecordHealthCheck(latency, err == nil)
	
	if err != nil {
		c.logger.Error("Health check failed", zap.Error(err), zap.Duration("latency", latency))
		return err
	}
	
	c.logger.Debug("Health check successful", zap.Duration("latency", latency))
	return nil
}

// CreateCollection creates a new vector collection
func (c *Client) CreateCollection(ctx context.Context, name string, dimension int) (*Collection, error) {
	start := time.Now()
	
	request := map[string]interface{}{
		"name": name,
		"config": map[string]interface{}{
			"params": map[string]interface{}{
				"vectors": map[string]interface{}{
					"size":     dimension,
					"distance": "Cosine",
				},
			},
		},
	}
	
	result, err := c.circuitBreaker.Execute(func() (interface{}, error) {
		resp, err := c.httpClient.R().
			SetContext(ctx).
			SetBody(request).
			Put(c.config.Endpoint + "/collections/" + name)
		
		if err != nil {
			return nil, err
		}
		
		if resp.StatusCode() >= 400 {
			return nil, fmt.Errorf("create collection failed: %s", resp.String())
		}
		
		var collection Collection
		if err := json.Unmarshal(resp.Body(), &collection); err != nil {
			return nil, fmt.Errorf("failed to parse response: %w", err)
		}
		
		return &collection, nil
	})
	
	latency := time.Since(start)
	c.metrics.RecordOperation("create_collection", latency, err == nil)
	
	if err != nil {
		c.logger.Error("Failed to create collection",
			zap.String("name", name),
			zap.Int("dimension", dimension),
			zap.Error(err))
		return nil, err
	}
	
	collection := result.(*Collection)
	c.logger.Info("Collection created successfully",
		zap.String("name", name),
		zap.Int("dimension", dimension),
		zap.Duration("latency", latency))
	
	return collection, nil
}