package main

import (
	"context"
	"fmt"
	"log"
	"time"

	rtdb "github.com/iceyxsm/rtdb-go"
)

func main() {
	// Create a production-optimized client configuration
	config := rtdb.DefaultConfig()
	config.Address = "localhost"
	config.Port = 6334
	config.EnableSIMDX = true
	config.BatchSize = 1000
	config.RequestTimeout = 30 * time.Second

	// Create client with production features
	client, err := rtdb.NewClient(config)
	if err != nil {
		log.Fatalf("Failed to create RTDB client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()

	// Create a collection optimized for SIMDX performance
	fmt.Println("Creating collection...")
	err = client.CreateCollection(ctx, "example_collection", 768)
	if err != nil {
		log.Fatalf("Failed to create collection: %v", err)
	}

	// Prepare vectors with SIMDX-friendly dimensions
	vectors := []rtdb.Vector{
		{
			ID:     "doc_1",
			Vector: generateVector(768),
			Metadata: map[string]interface{}{
				"category": "technology",
				"source":   "research_paper",
				"year":     2024,
			},
		},
		{
			ID:     "doc_2", 
			Vector: generateVector(768),
			Metadata: map[string]interface{}{
				"category": "science",
				"source":   "journal_article",
				"year":     2024,
			},
		},
	}

	// Insert vectors with batch optimization
	fmt.Println("Inserting vectors...")
	err = client.Insert(ctx, "example_collection", vectors)
	if err != nil {
		log.Fatalf("Failed to insert vectors: %v", err)
	}

	// Perform SIMDX-accelerated search
	fmt.Println("Performing search...")
	searchReq := &rtdb.SearchRequest{
		CollectionName: "example_collection",
		Vector:         generateVector(768),
		Limit:          10,
		WithPayload:    true,
		WithVector:     false,
		UseSIMDX:       true,
		BatchOptimize:  true,
		Filter: map[string]interface{}{
			"category": "technology",
		},
	}

	response, err := client.Search(ctx, searchReq)
	if err != nil {
		log.Fatalf("Failed to search: %v", err)
	}

	// Display results with performance metrics
	fmt.Printf("Found %d results\n", len(response.Results))
	if response.Metrics != nil {
		fmt.Printf("Query time: %v\n", response.Metrics.QueryTime)
		fmt.Printf("SIMDX accelerated: %v\n", response.Metrics.SIMDXAccelerated)
		fmt.Printf("Vectors scanned: %d\n", response.Metrics.VectorsScanned)
		fmt.Printf("Cache hits: %d\n", response.Metrics.CacheHits)
	}

	for i, result := range response.Results {
		fmt.Printf("Result %d: ID=%s, Score=%.4f\n", i+1, result.ID, result.Score)
		if result.Metadata != nil {
			fmt.Printf("  Metadata: %+v\n", result.Metadata)
		}
	}

	// Display client metrics
	metrics := client.GetMetrics()
	fmt.Printf("\nClient Metrics:\n")
	fmt.Printf("Total requests: %d\n", metrics.TotalRequests)
	fmt.Printf("Success rate: %.2f%%\n", metrics.SuccessRate())
	fmt.Printf("Average latency: %v\n", metrics.AverageLatency())
	fmt.Printf("Min latency: %v\n", metrics.MinLatency)
	fmt.Printf("Max latency: %v\n", metrics.MaxLatency)
}

// generateVector creates a random vector for testing
func generateVector(size int) []float32 {
	vector := make([]float32, size)
	for i := range vector {
		vector[i] = float32(i%100) / 100.0 // Simple pattern for demo
	}
	return vector
}

// GetMetrics is a method that should be added to the Client struct
func (c *rtdb.Client) GetMetrics() rtdb.MetricsSnapshot {
	return c.metrics.GetMetrics()
}