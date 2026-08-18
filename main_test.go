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
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest/observer"
)

func testRuntimeDependencies(t *testing.T) (RuntimeDependencies, *atomic.Int32, *atomic.Int32) {
	t.Helper()
	var closes atomic.Int32
	var syncs atomic.Int32
	store := &Store{
		ping: func(context.Context) error { return nil },
		close: func() error {
			closes.Add(1)
			return nil
		},
	}
	return RuntimeDependencies{
		InitLogger: func() {},
		SyncLogger: func() error {
			syncs.Add(1)
			return nil
		},
		OpenStore:     func(Config) (*Store, error) { return store, nil },
		LogError:      func(error) {},
		NewApp:        NewApp,
		NewHTTPServer: NewHTTPServer,
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
	dependencies.OpenStore = func(Config) (*Store, error) {
		return &Store{
			ping:  func(context.Context) error { return errors.New("redis unavailable") },
			close: func() error { closes.Add(1); return nil },
		}, nil
	}

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
	assert.Zero(t, signals.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenRedisOptionsAreInvalid(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.OpenStore = func(Config) (*Store, error) { return nil, errInvalidRedisURL }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Zero(t, closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenStoreOpenFailsAfterAllocation(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.OpenStore = func(Config) (*Store, error) {
		return &Store{
			close: func() error {
				closes.Add(1)
				return nil
			},
		}, errors.New("open failed")
	}

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenStoreIsMissing(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.OpenStore = func(Config) (*Store, error) { return nil, nil }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Zero(t, closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenAppIsMissing(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.NewApp = func(Config, *Store) *App { return nil }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRunApplicationCreatesSignalAfterRedisPingAndStopsBeforeResourceCleanup(t *testing.T) {
	var events []string
	dependencies, _, _ := testRuntimeDependencies(t)
	dependencies.InitLogger = func() { events = append(events, "logger") }
	dependencies.OpenStore = func(Config) (*Store, error) {
		events = append(events, "redis")
		return &Store{
			ping: func(context.Context) error {
				events = append(events, "ping")
				return nil
			},
			close: func() error {
				events = append(events, "redis-close")
				return nil
			},
		}, nil
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
	dependencies.NewApp = func(cfg Config, store *Store) *App {
		events = append(events, "router")
		return NewApp(cfg, store)
	}
	dependencies.NewHTTPServer = func(cfg Config, handler http.Handler) *HTTPServer {
		events = append(events, "server")
		return NewHTTPServer(Config{Port: "0", ShutdownTimeout: time.Second}, handler)
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
	redisCloseErr := errors.New("redis close failed")
	syncErr := errors.New("logger sync failed")
	var events []string
	dependencies, _, _ := testRuntimeDependencies(t)
	dependencies.SignalContext = func(parent context.Context) (context.Context, context.CancelFunc) {
		return context.WithCancel(parent)
	}
	dependencies.NewHTTPServer = func(cfg Config, handler http.Handler) *HTTPServer {
		server := NewHTTPServer(cfg, handler)
		server.listenAndServe = func() error { return runErr }
		return server
	}
	dependencies.OpenStore = func(Config) (*Store, error) {
		return &Store{
			ping: func(context.Context) error { return nil },
			close: func() error {
				events = append(events, "redis-close")
				return redisCloseErr
			},
		}, nil
	}
	dependencies.LogError = func(error) { events = append(events, "log") }
	dependencies.SyncLogger = func() error {
		events = append(events, "sync")
		return syncErr
	}

	err := RunApplication(t.Context(), defaultConfig(), dependencies)
	assert.ErrorIs(t, err, runErr)
	assert.ErrorIs(t, err, redisCloseErr)
	assert.ErrorIs(t, err, syncErr)
	assert.Equal(t, []string{"redis-close", "log", "sync"}, events)
}

func TestProductionRuntimeFailureLogDoesNotIncludeUnderlyingError(t *testing.T) {
	originalLogger := logger
	core, observed := observer.New(zap.ErrorLevel)
	logger = zap.New(core).Sugar()
	t.Cleanup(func() { logger = originalLogger })

	productionRuntimeDependencies().LogError(errors.New("redis://user:super-secret@cache.internal:6379/0"))

	entries := observed.All()
	if assert.Len(t, entries, 1) {
		assert.Equal(t, "application stopped", entries[0].Message)
		assert.Empty(t, entries[0].Context)
		assert.NotContains(t, entries[0].Message, "super-secret")
	}
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
