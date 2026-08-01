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

func testRuntimeDependencies(t *testing.T) (RuntimeDependencies, *atomic.Int32, *atomic.Int32) {
	t.Helper()
	var closes atomic.Int32
	var syncs atomic.Int32
	return RuntimeDependencies{
		InitLogger: func() {},
		SyncLogger: func() error {
			syncs.Add(1)
			return nil
		},
		InitRedis: func(Config) error { return nil },
		CloseRedis: func() error {
			closes.Add(1)
			return nil
		},
		CloseRequestLogger: func() error { return nil },
		LogError:           func(error) {},
		NewRouter:          func(Config, Dependencies) http.Handler { return http.NewServeMux() },
		NewHTTPServer:      NewHTTPServer,
		SignalContext: func(parent context.Context) (context.Context, context.CancelFunc) {
			return context.WithCancel(parent)
		},
	}, &closes, &syncs
}

func TestRuntimeExitCodeReturnsFailureAfterRedisStartupFailure(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	var signals atomic.Int32
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		signals.Add(1)
		return context.WithCancel(parent)
	}
	dependencies.PingRedis = func(context.Context) error { return errors.New("redis unavailable") }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
	assert.Zero(t, signals.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenRedisOptionsAreInvalid(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.InitRedis = func(Config) error { return errInvalidRedisURL }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Zero(t, closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRunApplicationCreatesSignalAfterRedisPingAndStopsBeforeResourceCleanup(t *testing.T) {
	var events []string
	dependencies, _, _ := testRuntimeDependencies(t)
	dependencies.InitLogger = func() { events = append(events, "logger") }
	dependencies.InitRedis = func(Config) error {
		events = append(events, "redis")
		return nil
	}
	dependencies.PingRedis = func(context.Context) error {
		events = append(events, "ping")
		return nil
	}
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		events = append(events, "signal")
		ctx, cancel := context.WithCancel(parent)
		cancel()
		return ctx, func() {
			events = append(events, "stop")
			cancel()
		}
	}
	dependencies.NewRouter = func(Config, Dependencies) http.Handler {
		events = append(events, "router")
		return http.NewServeMux()
	}
	dependencies.NewHTTPServer = func(cfg Config, handler http.Handler) *HTTPServer {
		events = append(events, "server")
		return NewHTTPServer(Config{Port: "0", ShutdownTimeout: time.Second}, handler)
	}
	dependencies.CloseRedis = func() error {
		events = append(events, "redis-close")
		return nil
	}
	dependencies.SyncLogger = func() error {
		events = append(events, "logger-sync")
		return nil
	}

	assert.NoError(t, RunApplication(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, []string{"logger", "redis", "ping", "signal", "router", "server", "stop", "redis-close", "logger-sync"}, events)
}

func TestRunApplicationJoinsCleanupErrorsAndLogsBeforeSync(t *testing.T) {
	runErr := errors.New("serve failed")
	requestCloseErr := errors.New("request logger close failed")
	redisCloseErr := errors.New("redis close failed")
	syncErr := errors.New("logger sync failed")
	var events []string
	dependencies, _, _ := testRuntimeDependencies(t)
	dependencies.PingRedis = func(context.Context) error { return nil }
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		return context.WithCancel(parent)
	}
	dependencies.NewHTTPServer = func(cfg Config, handler http.Handler) *HTTPServer {
		server := NewHTTPServer(cfg, handler)
		server.listenAndServe = func() error { return runErr }
		return server
	}
	dependencies.CloseRequestLogger = func() error {
		events = append(events, "request-close")
		return requestCloseErr
	}
	dependencies.CloseRedis = func() error {
		events = append(events, "redis-close")
		return redisCloseErr
	}
	dependencies.LogError = func(error) { events = append(events, "log") }
	dependencies.SyncLogger = func() error {
		events = append(events, "sync")
		return syncErr
	}

	err := RunApplication(t.Context(), defaultConfig(), dependencies)
	assert.ErrorIs(t, err, runErr)
	assert.ErrorIs(t, err, requestCloseErr)
	assert.ErrorIs(t, err, redisCloseErr)
	assert.ErrorIs(t, err, syncErr)
	assert.Equal(t, []string{"request-close", "redis-close", "log", "sync"}, events)
}

func TestRuntimeExitCodeReturnsFailureWhenHTTPCannotListen(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	dependencies, closes, syncs := testRuntimeDependencies(t)
	var signals, stops atomic.Int32
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		signals.Add(1)
		ctx, cancel := context.WithCancel(parent)
		return ctx, func() {
			stops.Add(1)
			cancel()
		}
	}
	dependencies.PingRedis = func(context.Context) error { return nil }
	cfg := defaultConfig()
	cfg.Port = strconv.Itoa(listener.Addr().(*net.TCPAddr).Port)

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), cfg, dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
	assert.Equal(t, int32(1), signals.Load())
	assert.Equal(t, int32(1), stops.Load())
}

func TestRuntimeExitCodeReturnsSuccessAfterNormalCancellation(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	var signals, stops atomic.Int32
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		signals.Add(1)
		ctx, cancel := context.WithCancel(parent)
		return ctx, func() {
			stops.Add(1)
			cancel()
		}
	}
	dependencies.PingRedis = func(context.Context) error { return nil }
	cfg := defaultConfig()
	cfg.Port = "0"
	cfg.ShutdownTimeout = time.Second

	ctx, cancel := context.WithCancel(t.Context())
	cancel()

	assert.Equal(t, runtimeSuccessExitCode, RuntimeExitCode(ctx, cfg, dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
	assert.Equal(t, int32(1), signals.Load())
	assert.Equal(t, int32(1), stops.Load())
}
