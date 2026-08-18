package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest/observer"
)

func TestLongToShortHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	store, client := newTestStore(t)
	cfg := defaultConfig()
	app := NewApp(cfg, store)
	router := gin.New()
	router.POST("/short", app.longToShortHandler())

	tests := []struct {
		name        string
		contentType string
		body        string
		wantCode    int
		wantShort   string
	}{
		{name: "form", contentType: "application/x-www-form-urlencoded", body: url.Values{"longUrl": {"https://example.com/a"}, "shortKey": {"form-key"}}.Encode(), wantCode: ResponseCodeSuccessLegacy, wantShort: "https://localhost:8080/form-key"},
		{name: "json", contentType: "application/json", body: `{"longUrl":"https://example.com/b","shortKey":"json-key"}`, wantCode: ResponseCodeSuccessLegacy, wantShort: "https://localhost:8080/json-key"},
		{name: "legacy base64", contentType: "application/x-www-form-urlencoded", body: url.Values{"longUrl": {"aHR0cHM6Ly9leGFtcGxlLmNvbQ=="}, "shortKey": {"base64-key"}}.Encode(), wantCode: ResponseCodeSuccessLegacy, wantShort: "https://localhost:8080/base64-key"},
		{name: "dangerous scheme", contentType: "application/json", body: `{"longUrl":"javascript:alert(1)","shortKey":"danger"}`, wantCode: ResponseCodeParamsCheckError},
		{name: "reserved short key", contentType: "application/json", body: `{"longUrl":"https://example.com","shortKey":"healthz"}`, wantCode: ResponseCodeParamsCheckError},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			before, err := client.DBSize(t.Context()).Result()
			require.NoError(t, err)

			request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(tt.body))
			request.Header.Set("Content-Type", tt.contentType)
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)

			assert.Equal(t, http.StatusOK, response.Code)
			var payload struct {
				Code     int
				ShortURL string `json:"ShortUrl"`
			}
			require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
			assert.Equal(t, tt.wantCode, payload.Code)
			assert.Equal(t, tt.wantShort, payload.ShortURL)

			if tt.wantCode == ResponseCodeParamsCheckError {
				after, err := client.DBSize(t.Context()).Result()
				require.NoError(t, err)
				assert.Equal(t, before, after)
			}
		})
	}

	keys, err := client.Keys(t.Context(), "*").Result()
	require.NoError(t, err)
	assert.NotContains(t, keys, "danger")
	assert.NotContains(t, keys, "healthz")
}

func TestLongToShortHandlerUsesBaseURL(t *testing.T) {
	gin.SetMode(gin.TestMode)
	store, _ := newTestStore(t)
	cfg := defaultConfig()
	cfg.BaseURL, _ = parseBaseURL("https://public.example/links/")
	app := NewApp(cfg, store)
	router := gin.New()
	router.POST("/short", app.longToShortHandler())

	response := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(`{"longUrl":"https://example.com","shortKey":"base-url"}`))
	request.Header.Set("Content-Type", "application/json")
	router.ServeHTTP(response, request)

	var payload struct {
		ShortURL string `json:"ShortUrl"`
	}
	require.NoError(t, json.NewDecoder(response.Body).Decode(&payload))
	assert.Equal(t, "https://public.example/links/base-url", payload.ShortURL)
}

func TestShortToLongHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	store, _ := newTestStore(t)
	app := NewApp(defaultConfig(), store)
	router := gin.New()
	router.GET("/:shortKey", app.shortToLongHandler())

	t.Run("redirects existing key", func(t *testing.T) {
		created, err := store.StoreShortURL(t.Context(), "found", "https://example.com/found", time.Hour)
		require.NoError(t, err)
		require.True(t, created)

		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/found", nil))

		assert.Equal(t, http.StatusMovedPermanently, response.Code)
		assert.Equal(t, "https://example.com/found", response.Header().Get("Location"))
	})

	t.Run("returns not found for missing key", func(t *testing.T) {
		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/missing", nil))

		assert.Equal(t, http.StatusNotFound, response.Code)
		assert.JSONEq(t, `{"Code":1002,"Message":"failed to get long URL, please check the short URL if exists or expired","Data":null}`, response.Body.String())
	})
}

func TestShortToLongHandlerReturnsInternalServerErrorWithoutRedisDetails(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	originalLogger := logger
	core, observed := observer.New(zap.WarnLevel)
	logger = zap.New(core).Sugar()
	t.Cleanup(func() { logger = originalLogger })

	server, err := miniredis.Run()
	require.NoError(t, err)
	store := NewStore(&redis.Options{Addr: server.Addr(), MaxRetries: 0})
	t.Cleanup(func() { _ = store.Close() })
	server.Close()

	app := NewApp(defaultConfig(), store)
	router := gin.New()
	router.GET("/:shortKey", app.shortToLongHandler())
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/redis-down", nil))

	assert.Equal(t, http.StatusInternalServerError, response.Code)
	var payload Response
	require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
	assert.Equal(t, ResponseCodeServerError, payload.Code)
	assert.Equal(t, "failed to get long URL", payload.Msg)
	assert.NotContains(t, payload.Msg, "connection refused")

	entries := observed.All()
	if assert.Len(t, entries, 1) {
		assert.Equal(t, "failed to resolve short URL", entries[0].Message)
		assert.Empty(t, entries[0].Context)
	}
}

func TestLongToShortHandlerDoesNotLogConflictingShortKey(t *testing.T) {
	gin.SetMode(gin.TestMode)
	originalLogger := logger
	core, observed := observer.New(zap.InfoLevel)
	logger = zap.New(core).Sugar()
	t.Cleanup(func() { logger = originalLogger })
	store, _ := newTestStore(t)

	const sensitiveShortKey = "private-short-code"
	created, err := store.StoreShortURL(t.Context(), sensitiveShortKey, "https://example.com/first", time.Hour)
	require.NoError(t, err)
	require.True(t, created)

	app := NewApp(defaultConfig(), store)
	router := gin.New()
	router.POST("/short", app.longToShortHandler())
	request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(url.Values{
		"longUrl":  {"https://example.com/second"},
		"shortKey": {sensitiveShortKey},
	}.Encode()))
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	assert.Equal(t, http.StatusOK, response.Code)
	assert.Empty(t, observed.All())
}
