package main

import (
	"context"
	"fmt"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

const healthPingTimeout = time.Second

// HealthHandler returns a dependency-backed health endpoint. Details from a
// failing dependency are deliberately never returned to the caller.
func HealthHandler(ping func(context.Context) error) gin.HandlerFunc {
	return func(c *gin.Context) {
		if ping == nil {
			c.JSON(http.StatusServiceUnavailable, gin.H{"status": "unavailable"})
			return
		}

		ctx, cancel := context.WithTimeout(c.Request.Context(), healthPingTimeout)
		defer cancel()
		if err := ping(ctx); err != nil {
			c.JSON(http.StatusServiceUnavailable, gin.H{"status": "unavailable"})
			return
		}

		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	}
}

// RunHealthcheck checks the local health endpoint and returns success only for
// an HTTP 200 response.
func RunHealthcheck(ctx context.Context, port string) error {
	client := &http.Client{Timeout: 3 * time.Second}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, "http://127.0.0.1:"+port+"/healthz", nil)
	if err != nil {
		return fmt.Errorf("create healthcheck request: %w", err)
	}
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("request healthcheck: %w", err)
	}
	defer response.Body.Close()

	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("healthcheck returned HTTP %d", response.StatusCode)
	}
	return nil
}
