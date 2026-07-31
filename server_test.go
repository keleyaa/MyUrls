package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"regexp"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func recordLifecycleEvent(sequence *atomic.Int64) int64 {
	return sequence.Add(1)
}

func TestStaticAssetsHaveNoRuntimeDependencies(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})

	tests := []struct {
		path        string
		contentType string
	}{
		{path: "/", contentType: "text/html"},
		{path: "/app.js", contentType: "javascript"},
		{path: "/styles.css", contentType: "text/css"},
		{path: "/logo.png", contentType: "image/png"},
		{path: "/healthz", contentType: "application/json"},
	}

	responses := make(map[string]string, len(tests))
	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			response := httptest.NewRecorder()
			router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, tt.path, nil))

			require.Equal(t, http.StatusOK, response.Code)
			assert.Contains(t, response.Header().Get("Content-Type"), tt.contentType)
			responses[tt.path] = response.Body.String()
		})
	}

	document := responses["/"]
	lowerDocument := strings.ToLower(document)
	for _, forbidden := range []string{"unpkg.com", "jsdelivr.net", "vue", "axios", "element-ui"} {
		assert.NotContains(t, lowerDocument, forbidden)
	}

	for _, required := range []string{
		`id="long-url"`,
		`id="short-key"`,
		`id="short-url"`,
		`id="shorten-button"`,
		`id="copy-button"`,
		`id="status"`,
	} {
		assert.Contains(t, document, required)
	}
	statusTag := regexp.MustCompile(`<[^>]+id="status"[^>]*>`).FindString(document)
	require.NotEmpty(t, statusTag)
	assert.Contains(t, statusTag, `role="status"`)

	assert.Contains(t, document, `<html lang="zh-CN">`)
	assert.Contains(t, document, `<main`)
	assert.Contains(t, document, `<form`)
	assert.Contains(t, document, `<label for="long-url">`)
	assert.Contains(t, document, `<label for="short-key">`)
	assert.Contains(t, document, `<label for="short-url">`)
	assert.Contains(t, document, `pattern="[A-Za-z0-9_\-]{1,64}"`)
	assert.Contains(t, document, `href="/styles.css"`)
	assert.Contains(t, document, `src="/app.js"`)
	assert.Contains(t, document, `src="/logo.png"`)
	assert.Contains(t, document, `src="/app.js" defer`)
	assert.Equal(t, 1, strings.Count(lowerDocument, `<script`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `rel="stylesheet"`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `<img`))
	assert.NotContains(t, lowerDocument, `@font-face`)
	assert.NotContains(t, lowerDocument, `url(http`)

	appScript := strings.ToLower(responses["/app.js"])
	for _, forbidden := range []string{"btoa(", "unpkg.com", "jsdelivr.net", "vue", "axios", "element-ui"} {
		assert.NotContains(t, appScript, forbidden)
	}
	assert.Contains(t, appScript, "new formdata()")
	assert.Contains(t, appScript, "fetch('/short'")
	assert.Contains(t, appScript, "navigator.clipboard")
	assert.Contains(t, appScript, "document.execcommand('copy')")
	automaticCopyIndex := strings.Index(appScript, "await copytext(shorturl)")
	enableManualCopyIndex := strings.Index(appScript, "copybutton.disabled = false")
	require.NotEqual(t, -1, automaticCopyIndex)
	require.NotEqual(t, -1, enableManualCopyIndex)
	assert.Less(t, automaticCopyIndex, enableManualCopyIndex)
	assert.Contains(t, appScript, "} finally {\n        copybutton.disabled = false\n      }")

	styles := strings.ToLower(responses["/styles.css"])
	assert.Contains(t, styles, "width: min(42rem, calc(100% - 2rem))")
	assert.Contains(t, styles, "outline: 3px solid #2456a6")
	assert.NotContains(t, styles, "#79a9f5")
	for _, forbidden := range []string{"@import", "@font-face", "linear-gradient", "radial-gradient", "vw;", "url(http"} {
		assert.NotContains(t, styles, forbidden)
	}

	routes := make(map[string]bool)
	for _, route := range router.Routes() {
		routes[route.Method+" "+route.Path] = true
	}
	assert.True(t, routes[http.MethodGet+" /healthz"])
	assert.True(t, routes[http.MethodGet+" /:shortKey"])
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
