package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"strconv"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func recordLifecycleEvent(sequence *atomic.Int64) int64 {
	return sequence.Add(1)
}

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
	shutdownStarted := make(chan struct{})
	release := make(chan struct{})
	handlerFinished := make(chan struct{})
	var eventSequence atomic.Int64
	var handlerFinishedAt atomic.Int64
	var serveReturnedAt atomic.Int64
	server := NewHTTPServer(defaultConfig(), http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer func() {
			handlerFinishedAt.Store(recordLifecycleEvent(&eventSequence))
			close(handlerFinished)
		}()
		close(started)
		<-release
		w.WriteHeader(http.StatusOK)
	}))
	server.listenAndServe = func() error { return server.Server.Serve(listener) }
	server.shutdown = func(ctx context.Context) error {
		close(shutdownStarted)
		return server.Server.Shutdown(ctx)
	}

	ctx, cancel := context.WithCancel(t.Context())
	result := make(chan error, 1)
	go func() {
		err := server.Serve(ctx)
		serveReturnedAt.Store(recordLifecycleEvent(&eventSequence))
		result <- err
	}()

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
	<-shutdownStarted

	close(release)
	<-handlerFinished
	<-requestDone
	assert.NoError(t, <-result)
	assert.Less(t, handlerFinishedAt.Load(), serveReturnedAt.Load(), "Serve returned before the in-flight handler completed")
}

func TestServeReturnsShutdownDeadlineWithoutLeakingHandler(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	started := make(chan struct{})
	release := make(chan struct{})
	finished := make(chan struct{})
	clientDone := make(chan struct{})
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
		defer close(clientDone)
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
	<-clientDone
}

func TestServeDoesNotIgnoreUnexpectedListenError(t *testing.T) {
	want := errors.New("listener failure")
	server := NewHTTPServer(defaultConfig(), http.NewServeMux())
	server.listenAndServe = func() error { return want }

	assert.ErrorIs(t, server.Serve(t.Context()), want)
}

func TestServeJoinsShutdownAndListenErrorsWhenCancellationRaces(t *testing.T) {
	shutdownErr := errors.New("shutdown failure")
	listenErr := errors.New("listener failure")
	shutdownEntered := make(chan struct{})
	server := NewHTTPServer(defaultConfig(), http.NewServeMux())
	server.shutdown = func(context.Context) error {
		close(shutdownEntered)
		return shutdownErr
	}
	server.listenAndServe = func() error {
		<-shutdownEntered
		return listenErr
	}

	ctx, cancel := context.WithCancel(t.Context())
	result := make(chan error, 1)
	go func() { result <- server.Serve(ctx) }()
	cancel()

	err := <-result
	assert.ErrorIs(t, err, shutdownErr)
	assert.ErrorIs(t, err, listenErr)
}
