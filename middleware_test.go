package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/time/rate"
)

func TestAuthMiddleware(t *testing.T) {
	gin.SetMode(gin.TestMode)

	tests := []struct {
		name          string
		token         string
		authorization string
		wantStatus    int
		wantCode      int
		wantChallenge string
	}{
		{name: "empty token allows requests", wantStatus: http.StatusNoContent},
		{name: "missing bearer token is unauthorized", token: "secret", wantStatus: http.StatusUnauthorized, wantCode: ResponseCodeUnauthorized, wantChallenge: `Bearer realm="MyUrls"`},
		{name: "wrong bearer token is unauthorized", token: "secret", authorization: "Bearer wrong", wantStatus: http.StatusUnauthorized, wantCode: ResponseCodeUnauthorized, wantChallenge: `Bearer realm="MyUrls", error="invalid_token"`},
		{name: "different length token is unauthorized", token: "secret", authorization: "Bearer much-longer-wrong-token", wantStatus: http.StatusUnauthorized, wantCode: ResponseCodeUnauthorized, wantChallenge: `Bearer realm="MyUrls", error="invalid_token"`},
		{name: "correct bearer token allows request", token: "secret", authorization: "Bearer secret", wantStatus: http.StatusNoContent},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			router := gin.New()
			router.Use(AuthMiddleware(tt.token))
			router.GET("/", func(c *gin.Context) { c.Status(http.StatusNoContent) })

			request := httptest.NewRequest(http.MethodGet, "/", nil)
			if tt.authorization != "" {
				request.Header.Set("Authorization", tt.authorization)
			}
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)

			assert.Equal(t, tt.wantStatus, response.Code)
			assert.Equal(t, tt.wantChallenge, response.Header().Get("WWW-Authenticate"))
			if tt.wantCode != 0 {
				var payload Response
				require.NoError(t, json.NewDecoder(response.Body).Decode(&payload))
				assert.Equal(t, tt.wantCode, payload.Code)
			}
		})
	}
}

func TestRateLimitMiddleware(t *testing.T) {
	gin.SetMode(gin.TestMode)

	t.Run("nil limiter allows requests", func(t *testing.T) {
		router := gin.New()
		router.Use(RateLimitMiddleware(nil))
		router.GET("/", func(c *gin.Context) { c.Status(http.StatusNoContent) })

		response := httptest.NewRecorder()
		router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/", nil))
		assert.Equal(t, http.StatusNoContent, response.Code)
	})

	t.Run("second request is rate limited without replenishment", func(t *testing.T) {
		router := gin.New()
		router.Use(RateLimitMiddleware(rate.NewLimiter(0, 1)))
		router.GET("/", func(c *gin.Context) { c.Status(http.StatusNoContent) })

		first := httptest.NewRecorder()
		router.ServeHTTP(first, httptest.NewRequest(http.MethodGet, "/", nil))
		assert.Equal(t, http.StatusNoContent, first.Code)

		second := httptest.NewRecorder()
		router.ServeHTTP(second, httptest.NewRequest(http.MethodGet, "/", nil))
		assert.Equal(t, http.StatusTooManyRequests, second.Code)
		var payload Response
		require.NoError(t, json.NewDecoder(second.Body).Decode(&payload))
		assert.Equal(t, ResponseCodeRateLimited, payload.Code)
	})
}

func TestBodyLimitMiddleware(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	cfg := defaultConfig()
	cfg.MaxBodyBytes = 64
	router := NewRouter(cfg, Dependencies{})
	validJSON := `{"longUrl":"https://example.com/long"}`

	tests := []struct {
		name          string
		body          string
		unknownLength bool
	}{
		{name: "valid JSON followed by whitespace", body: validJSON + strings.Repeat(" ", 1024)},
		{name: "valid JSON followed by a second JSON value", body: validJSON + validJSON + strings.Repeat(" ", 1024)},
		{name: "chunked request with unknown content length", body: validJSON + strings.Repeat(" ", 1024), unknownLength: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(tt.body))
			request.Header.Set("Content-Type", "application/json")
			if tt.unknownLength {
				request.ContentLength = -1
				request.TransferEncoding = []string{"chunked"}
			}
			response := httptest.NewRecorder()
			router.ServeHTTP(response, request)

			assert.Equal(t, http.StatusOK, response.Code)
			var payload Response
			require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
			assert.Equal(t, ResponseCodeParamsCheckError, payload.Code)

			size, err := GetRedisClient().DBSize(t.Context()).Result()
			require.NoError(t, err)
			assert.Zero(t, size)
		})
	}
}

func TestBodyLimitMiddlewareReadsCompleteBodyBeforeCallingHandler(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.Use(BodyLimitMiddleware(64))
	router.POST("/", func(c *gin.Context) {
		buffer := make([]byte, 1)
		_, _ = c.Request.Body.Read(buffer)
		c.Status(http.StatusNoContent)
	})

	request := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{"longUrl":"https://example.com/long"}`+strings.Repeat(" ", 1024)))
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	assert.Equal(t, http.StatusOK, response.Code)
	var payload Response
	require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
	assert.Equal(t, ResponseCodeParamsCheckError, payload.Code)
}

func TestBodyLimitMiddlewarePreservesFormRequests(t *testing.T) {
	gin.SetMode(gin.TestMode)
	InitLogger()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	cfg := defaultConfig()
	cfg.MaxBodyBytes = 64
	router := NewRouter(cfg, Dependencies{})
	body := url.Values{"longUrl": {"https://e.co"}, "shortKey": {"form"}}.Encode()
	request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(body))
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	assert.Equal(t, http.StatusOK, response.Code)
	var payload struct{ Code int }
	require.NoError(t, json.NewDecoder(response.Body).Decode(&payload))
	assert.Equal(t, ResponseCodeSuccessLegacy, payload.Code)
}
