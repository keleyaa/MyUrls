package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
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

func TestManropeFontIsServedLocally(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(
		http.MethodGet,
		"/fonts/manrope-latin-wght-normal.woff2",
		nil,
	))
	require.Equal(t, http.StatusOK, response.Code)
	assert.Contains(t, response.Header().Get("Content-Type"), "font/woff2")
	assert.True(t, strings.HasPrefix(response.Body.String(), "wOF2"))
	license, err := os.ReadFile("public/fonts/OFL.txt")
	require.NoError(t, err)
	assert.Contains(t, string(license), "SIL OPEN FONT LICENSE Version 1.1")
}

func TestLuminousFocusDocumentContract(t *testing.T) {
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
	for _, required := range []string{
		`<html lang="zh-CN">`,
		`id="page-title"`,
		`>MyUrls<span aria-hidden="true">.</span></h1>`,
		`把长链接，变得简单。`,
		`id="shorten-form"`,
		`id="long-url"`,
		`id="shorten-button"`,
		`aria-label="生成短链接"`,
		`<svg class="submit-arrow"`,
		`<details class="custom-key">`,
		`<summary>`,
		`id="short-key"`,
		`id="copy-button"`,
		`id="short-url"`,
		`id="status"`,
		`role="status"`,
		`aria-live="polite"`,
		`href="https://github.com/keleyaa/MyUrls"`,
		`target="_blank"`,
		`rel="noopener noreferrer"`,
		`Go · MIT`,
	} {
		assert.Contains(t, document, required)
	}

	statusTag := regexp.MustCompile(`<[^>]+id="status"[^>]*>`).FindString(document)
	require.NotEmpty(t, statusTag)
	assert.Contains(t, statusTag, `role="status"`)

	assert.Equal(t, 1, strings.Count(lowerDocument, `<script`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `<h1`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `rel="stylesheet"`))
	assert.Equal(t, 0, strings.Count(lowerDocument, `<img`))
	for _, forbidden := range []string{"logo.png", "fonts.googleapis.com", "fonts.gstatic.com", "api.github.com"} {
		assert.NotContains(t, lowerDocument, forbidden)
	}
	assert.Equal(t, 1, len(regexp.MustCompile(`(?:src|href)="https?://`).FindAllString(document, -1)))

	copyButtonTag := regexp.MustCompile(`<button[^>]+id="copy-button"[^>]*>`).FindString(document)
	require.NotEmpty(t, copyButtonTag)
	assert.Contains(t, copyButtonTag, `hidden`)
	assert.Contains(t, copyButtonTag, `disabled`)

	routes := make(map[string]bool)
	for _, route := range router.Routes() {
		routes[route.Method+" "+route.Path] = true
	}
	assert.True(t, routes[http.MethodGet+" /healthz"])
	assert.True(t, routes[http.MethodGet+" /:shortKey"])
	assert.False(t, routes[http.MethodGet+" /logo.png"])
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
