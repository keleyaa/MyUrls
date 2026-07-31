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
		InitRedis: func(Config) {},
		CloseRedis: func() error {
			closes.Add(1)
			return nil
		},
		NewRouter:     func(Config, Dependencies) http.Handler { return http.NewServeMux() },
		NewHTTPServer: NewHTTPServer,
	}, &closes, &syncs
}

func TestRuntimeExitCodeReturnsFailureAfterRedisStartupFailure(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.PingRedis = func(context.Context) error { return errors.New("redis unavailable") }

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), defaultConfig(), dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRuntimeExitCodeReturnsFailureWhenHTTPCannotListen(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	require.NoError(t, err)
	defer listener.Close()

	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.PingRedis = func(context.Context) error { return nil }
	cfg := defaultConfig()
	cfg.Port = strconv.Itoa(listener.Addr().(*net.TCPAddr).Port)

	assert.Equal(t, runtimeFailureExitCode, RuntimeExitCode(t.Context(), cfg, dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}

func TestRuntimeExitCodeReturnsSuccessAfterNormalCancellation(t *testing.T) {
	dependencies, closes, syncs := testRuntimeDependencies(t)
	dependencies.PingRedis = func(context.Context) error { return nil }
	cfg := defaultConfig()
	cfg.Port = "0"
	cfg.ShutdownTimeout = time.Second

	ctx, cancel := context.WithCancel(t.Context())
	cancel()

	assert.Equal(t, runtimeSuccessExitCode, RuntimeExitCode(ctx, cfg, dependencies))
	assert.Equal(t, int32(1), closes.Load())
	assert.Equal(t, int32(1), syncs.Load())
}
