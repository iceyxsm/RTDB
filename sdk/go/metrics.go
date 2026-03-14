package rtdb

import (
	"sync/atomic"
	"time"

	"github.com/prometheus/client_golang/prometheus"
)

// Metrics holds client performance metrics
type Metrics struct {
	requestsTotal   *prometheus.CounterVec
	requestDuration *prometheus.HistogramVec
	errorsTotal     *prometheus.CounterVec
	
	// Internal counters
	totalRequests int64
	totalErrors   int64
	minLatency    time.Duration
	maxLatency    time.Duration
	totalLatency  time.Duration
}

// NewMetrics creates a new metrics instance
func NewMetrics() *Metrics {
	return &Metrics{
		requestsTotal: prometheus.NewCounterVec(
			prometheus.CounterOpts{
				Name: "rtdb_client_requests_total",
				Help: "Total number of requests made to RTDB",
			},
			[]string{"method", "status"},
		),
		requestDuration: prometheus.NewHistogramVec(
			prometheus.HistogramOpts{
				Name:    "rtdb_client_request_duration_seconds",
				Help:    "Request duration in seconds",
				Buckets: prometheus.DefBuckets,
			},
			[]string{"method"},
		),
		errorsTotal: prometheus.NewCounterVec(
			prometheus.CounterOpts{
				Name: "rtdb_client_errors_total",
				Help: "Total number of errors",
			},
			[]string{"method", "error_type"},
		),
		minLatency: time.Duration(0),
		maxLatency: time.Duration(0),
	}
}

// RecordRequest records a successful request
func (m *Metrics) RecordRequest(method string, statusCode int, duration time.Duration) {
	atomic.AddInt64(&m.totalRequests, 1)
	atomic.AddInt64((*int64)(&m.totalLatency), int64(duration))
	
	// Update min/max latency
	if m.minLatency == 0 || duration < m.minLatency {
		m.minLatency = duration
	}
	if duration > m.maxLatency {
		m.maxLatency = duration
	}
	
	m.requestsTotal.WithLabelValues(method, "success").Inc()
	m.requestDuration.WithLabelValues(method).Observe(duration.Seconds())
}

// RecordError records an error
func (m *Metrics) RecordError(method string, errorType string) {
	atomic.AddInt64(&m.totalErrors, 1)
	m.errorsTotal.WithLabelValues(method, errorType).Inc()
}

// MetricsSnapshot represents a snapshot of metrics
type MetricsSnapshot struct {
	TotalRequests int64
	TotalErrors   int64
	MinLatency    time.Duration
	MaxLatency    time.Duration
	TotalLatency  time.Duration
}

// GetMetrics returns a snapshot of current metrics
func (m *Metrics) GetMetrics() MetricsSnapshot {
	return MetricsSnapshot{
		TotalRequests: atomic.LoadInt64(&m.totalRequests),
		TotalErrors:   atomic.LoadInt64(&m.totalErrors),
		MinLatency:    m.minLatency,
		MaxLatency:    m.maxLatency,
		TotalLatency:  time.Duration(atomic.LoadInt64((*int64)(&m.totalLatency))),
	}
}

// SuccessRate calculates the success rate
func (ms MetricsSnapshot) SuccessRate() float64 {
	if ms.TotalRequests == 0 {
		return 100.0
	}
	successRequests := ms.TotalRequests - ms.TotalErrors
	return float64(successRequests) / float64(ms.TotalRequests) * 100.0
}

// AverageLatency calculates the average latency
func (ms MetricsSnapshot) AverageLatency() time.Duration {
	if ms.TotalRequests == 0 {
		return 0
	}
	return time.Duration(int64(ms.TotalLatency) / ms.TotalRequests)
}
// RecordHealthCheck records a health check result
func (m *Metrics) RecordHealthCheck(duration time.Duration, success bool) {
	method := "health_check"
	if success {
		m.RecordRequest(method, 200, duration)
	} else {
		m.RecordError(method, "health_check_failed")
	}
}

// RecordOperation records a general operation
func (m *Metrics) RecordOperation(operation string, duration time.Duration, success bool) {
	if success {
		m.RecordRequest(operation, 200, duration)
	} else {
		m.RecordError(operation, "operation_failed")
	}
}