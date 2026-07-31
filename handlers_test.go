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
)

func TestLongToShortHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	cfg := defaultConfig()
	router := gin.New()
	router.POST("/short", LongToShortHandler(cfg))

	tests := []struct {
		name        string
		contentType string
		body        string
		wantCode    int
		wantShort   string
	}{
		{
			name:        "form",
			contentType: "application/x-www-form-urlencoded",
			body:        url.Values{"longUrl": {"https://example.com/a"}, "shortKey": {"form-key"}}.Encode(),
			wantCode:    ResponseCodeSuccessLegacy,
			wantShort:   "https://localhost:8080/form-key",
		},
		{
			name:        "json",
			contentType: "application/json",
			body:        `{"longUrl":"https://example.com/b","shortKey":"json-key"}`,
			wantCode:    ResponseCodeSuccessLegacy,
			wantShort:   "https://localhost:8080/json-key",
		},
		{
			name:        "legacy base64",
			contentType: "application/x-www-form-urlencoded",
			body:        url.Values{"longUrl": {"aHR0cHM6Ly9leGFtcGxlLmNvbQ=="}, "shortKey": {"base64-key"}}.Encode(),
			wantCode:    ResponseCodeSuccessLegacy,
			wantShort:   "https://localhost:8080/base64-key",
		},
		{
			name:        "dangerous scheme",
			contentType: "application/json",
			body:        `{"longUrl":"javascript:alert(1)","shortKey":"danger"}`,
			wantCode:    ResponseCodeParamsCheckError,
		},
		{
			name:        "reserved short key",
			contentType: "application/json",
			body:        `{"longUrl":"https://example.com","shortKey":"healthz"}`,
			wantCode:    ResponseCodeParamsCheckError,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			before, err := GetRedisClient().DBSize(t.Context()).Result()
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
				after, err := GetRedisClient().DBSize(t.Context()).Result()
				require.NoError(t, err)
				assert.Equal(t, before, after)
			}
		})
	}

	keys, err := GetRedisClient().Keys(t.Context(), "*").Result()
	require.NoError(t, err)
	assert.NotContains(t, keys, "danger")
	assert.NotContains(t, keys, "healthz")
}

func TestShortToLongHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	router := gin.New()
	router.GET("/:shortKey", ShortToLongHandler())

	t.Run("redirects existing key", func(t *testing.T) {
		require.NoError(t, LongToShort(t.Context(), &LongToShortOptions{
			ShortKey:   "found",
			URL:        "https://example.com/found",
			expiration: time.Hour,
		}))

		request := httptest.NewRequest(http.MethodGet, "/found", nil)
		response := httptest.NewRecorder()
		router.ServeHTTP(response, request)

		assert.Equal(t, http.StatusMovedPermanently, response.Code)
		assert.Equal(t, "https://example.com/found", response.Header().Get("Location"))
	})

	t.Run("returns not found for missing key", func(t *testing.T) {
		request := httptest.NewRequest(http.MethodGet, "/missing", nil)
		response := httptest.NewRecorder()
		router.ServeHTTP(response, request)

		assert.Equal(t, http.StatusNotFound, response.Code)
		assert.JSONEq(t, `{"Code":1002,"Message":"failed to get long URL, please check the short URL if exists or expired","Data":null}`, response.Body.String())
	})
}

func TestShortToLongHandlerReturnsInternalServerErrorWithoutRedisDetails(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	resetRedisClient(t)

	server, err := miniredis.Run()
	require.NoError(t, err)
	initRedisClient(&redis.Options{Addr: server.Addr(), MaxRetries: 0})
	server.Close()

	router := gin.New()
	router.GET("/:shortKey", ShortToLongHandler())
	request := httptest.NewRequest(http.MethodGet, "/redis-down", nil)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	assert.Equal(t, http.StatusInternalServerError, response.Code)
	var payload Response
	require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
	assert.Equal(t, ResponseCodeServerError, payload.Code)
	assert.Equal(t, "failed to get long URL", payload.Msg)
	assert.NotContains(t, payload.Msg, "connection refused")
}
