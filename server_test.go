package main

import (
	"net/http"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestNewHTTPServerConfiguresAddressAndTimeouts(t *testing.T) {
	cfg := Config{
		Port:              "9123",
		ReadHeaderTimeout: time.Second,
		ReadTimeout:       2 * time.Second,
		WriteTimeout:      3 * time.Second,
		IdleTimeout:       4 * time.Second,
		ShutdownTimeout:   5 * time.Second,
	}

	server := NewHTTPServer(cfg, http.NewServeMux())

	assert.Equal(t, ":9123", server.Addr)
	assert.Equal(t, cfg.ReadHeaderTimeout, server.ReadHeaderTimeout)
	assert.Equal(t, cfg.ReadTimeout, server.ReadTimeout)
	assert.Equal(t, cfg.WriteTimeout, server.WriteTimeout)
	assert.Equal(t, cfg.IdleTimeout, server.IdleTimeout)
	assert.Equal(t, cfg.ShutdownTimeout, server.ShutdownTimeout)
}
