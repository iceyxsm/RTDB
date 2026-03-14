package rtdb

import (
	"sync"
	"sync/atomic"
	"time"
)

// ClientMetrics tracks client performance metrics
type ClientMetrics struct {
	mu                    sync.RWMutex
	totalRequests         int64
	successfulRequests    int64
	failedRequests        int64
	circuitBreakerOpens   int64
	totalLatency          int64
	minLatency            int64
	maxLatency            int64
	latencyBuckets        []int64 // Histogram buckets
	bucketBoundaries      []time.Duration
}

// NewClientMetrics creates a new metrics collector
func NewClientMetrics() *ClientMetrics {
	return &ClientMetrics{
		minLatency: int64(^uint64(0) >> 1), // Max int64
		bucketBoundaries: []time.Duration{
			time.Millisecond,
			5 * time.Millisecond,
			10 * time.Millisecond,
			25 * time.Millisecond,
			50 * time.Millisecond,
			100 * time.Millisecond,
			250 * time.Millisecond,
			500 * time.Millisecond,
			time.Second,
			5 * time.Second,
		},
		latencyBuckets: make([]int64, 11), // 10 buckets + overflow
	}
}

// RecordRequest records a request with its latency and success status
func (m *ClientMetrics) RecordRequest(latency time.Duration, success bool) {
	atomic.AddInt64(&m.totalRequests, 1)
	
	if success {
		atomic.AddInt64(&m.successfulRequests, 1)
	} else {
		atomic.AddInt64(&m.failedRequests, 1)
	}

	latencyNs := latency.Nanoseconds()
	atomic.AddInt64(&m.totalLatency, latencyNs)

	// Update min latency
	for {
		current := atomic.LoadInt64(&m.minLatency)
		if latencyNs >= current || atomic.CompareAndSwapInt64(&m.minLatency, current, latencyNs) {
			break
		}
	}

	// Update max latency
	for {
		current := atomic.LoadInt64(&m.maxLatency)
		if latencyNs <= current || atomic.CompareAndSwapInt64(&m.maxLatency, current, latencyNs) {
			break
		}
	}

	// Update histogram
	bucketIndex := len(m.bucketBoundaries) // Default to overflow bucket
	for i, boundary := range m.bucketBoundaries {
		if latency <= boundary {
			bucketIndex = i
			break
		}
	}
	atomic.AddInt64(&m.latencyBuckets[bucketIndex], 1)
}

// IncrementCircuitBreakerOpen increments the circuit breaker open counter
func (m *ClientMetrics) IncrementCircuitBreakerOpen() {
	atomic.AddInt64(&m.circuitBreakerOpens, 1)
}

// GetMetrics returns a snapshot of current metrics
func (m *ClientMetrics) GetMetrics() MetricsSnapshot {
	return MetricsSnapshot{
		TotalRequests:       atomic.LoadInt64(&m.totalRequests),
		SuccessfulRequests:  atomic.LoadInt64(&m.successfulRequests),
		FailedRequests:      atomic.LoadInt64(&m.failedRequests),
		CircuitBreakerOpens: atomic.LoadInt64(&m.circuitBreakerOpens),
		TotalLatency:        time.Duration(atomic.LoadInt64(&m.totalLatency)),
		MinLatency:          time.Duration(atomic.LoadInt64(&m.minLatency)),
		MaxLatency:          time.Duration(atomic.LoadInt64(&m.maxLatency)),
		LatencyBuckets:      m.copyLatencyBuckets(),
		BucketBoundaries:    m.bucketBoundaries,
	}
}

// copyLatencyBuckets creates a copy of the latency buckets
func (m *ClientMetrics) copyLatencyBuckets() []int64 {
	buckets := make([]int64, len(m.latencyBuckets))
	for i := range m.latencyBuckets {
		buckets[i] = atomic.LoadInt64(&m.latencyBuckets[i])
	}
	return buckets
}

// MetricsSnapshot represents a point-in-time snapshot of metrics
type MetricsSnapshot struct {
	TotalRequests       int64
	SuccessfulRequests  int64
	FailedRequests      int64
	CircuitBreakerOpens int64
	TotalLatency        time.Duration
	MinLatency          time.Duration
	MaxLatency          time.Duration
	LatencyBuckets      []int64
	BucketBoundaries    []time.Duration
}

// SuccessRate returns the success rate as a percentage
func (s MetricsSnapshot) SuccessRate() float64 {
	if s.TotalRequests == 0 {
		return 0
	}
	return float64(s.SuccessfulRequests) / float64(s.TotalRequests) * 100
}

// AverageLatency returns the average latency
func (s MetricsSnapshot) AverageLatency() time.Duration {
	if s.TotalRequests == 0 {
		return 0
	}
	return time.Duration(int64(s.TotalLatency) / s.TotalRequests)
}

// Reset resets all metrics to zero
func (m *ClientMetrics) Reset() {
	atomic.StoreInt64(&m.totalRequests, 0)
	atomic.StoreInt64(&m.successfulRequests, 0)
	atomic.StoreInt64(&m.failedRequests, 0)
	atomic.StoreInt64(&m.circuitBreakerOpens, 0)
	atomic.StoreInt64(&m.totalLatency, 0)
	atomic.StoreInt64(&m.minLatency, int64(^uint64(0)>>1))
	atomic.StoreInt64(&m.maxLatency, 0)
	
	for i := range m.latencyBuckets {
		atomic.StoreInt64(&m.latencyBuckets[i], 0)
	}
}