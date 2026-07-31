package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"strconv"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
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

func TestServeReturnsListenFailure(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	cfg := defaultConfig()
	cfg.Port = strconv.Itoa(listener.Addr().(*net.TCPAddr).Port)
	server := NewHTTPServer(cfg, http.NewServeMux())

	assert.Error(t, server.Serve(t.Context()))
}

func TestServeGracefullyWaitsForInflightRequest(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	started := make(chan struct{})
	release := make(chan struct{})
	finished := make(chan struct{})
	server := NewHTTPServer(defaultConfig(), http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		close(started)
		<-release
		close(finished)
		w.WriteHeader(http.StatusOK)
	}))
	server.listenAndServe = func() error { return server.Server.Serve(listener) }

	ctx, cancel := context.WithCancel(t.Context())
	result := make(chan error, 1)
	go func() { result <- server.Serve(ctx) }()

	requestDone := make(chan struct{})
	go func() {
		defer close(requestDone)
		response, err := http.Get("http://" + listener.Addr().String())
		if err == nil {
			response.Body.Close()
		}
	}()
	<-started
	cancel()

	select {
	case err := <-result:
		t.Fatalf("Serve returned before the in-flight request completed: %v", err)
	default:
	}

	close(release)
	<-finished
	<-requestDone
	assert.NoError(t, <-result)
}

func TestServeReturnsShutdownDeadlineWithoutLeakingHandler(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	started := make(chan struct{})
	release := make(chan struct{})
	finished := make(chan struct{})
	cfg := defaultConfig()
	cfg.ShutdownTimeout = 10 * time.Millisecond
	server := NewHTTPServer(cfg, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		close(started)
		<-release
		close(finished)
	}))
	server.listenAndServe = func() error { return server.Server.Serve(listener) }

	ctx, cancel := context.WithCancel(t.Context())
	result := make(chan error, 1)
	go func() { result <- server.Serve(ctx) }()

	go func() {
		response, err := http.Get("http://" + listener.Addr().String())
		if err == nil {
			response.Body.Close()
		}
	}()
	<-started
	cancel()

	select {
	case err := <-result:
		assert.ErrorIs(t, err, context.DeadlineExceeded)
	case <-time.After(time.Second):
		t.Fatal("Serve did not return after Shutdown timeout")
	}
	close(release)
	<-finished
}

func TestServeDoesNotIgnoreUnexpectedListenError(t *testing.T) {
	want := errors.New("listener failure")
	server := NewHTTPServer(defaultConfig(), http.NewServeMux())
	server.listenAndServe = func() error { return want }

	assert.ErrorIs(t, server.Serve(t.Context()), want)
}
