package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
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
	}{
		{name: "empty token allows requests", wantStatus: http.StatusNoContent},
		{name: "missing bearer token is unauthorized", token: "secret", wantStatus: http.StatusUnauthorized, wantCode: ResponseCodeUnauthorized},
		{name: "wrong bearer token is unauthorized", token: "secret", authorization: "Bearer wrong", wantStatus: http.StatusUnauthorized, wantCode: ResponseCodeUnauthorized},
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

	t.Run("second immediate request is rate limited", func(t *testing.T) {
		router := gin.New()
		router.Use(RateLimitMiddleware(rate.NewLimiter(1, 1)))
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

	router := NewRouter(Config{MaxBodyBytes: 16}, Dependencies{})

	request := httptest.NewRequest(http.MethodPost, "/short", strings.NewReader(`{"longUrl":"https://example.com/long"}`))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)

	assert.Equal(t, http.StatusOK, response.Code)
	var payload Response
	require.NoError(t, json.NewDecoder(bytes.NewReader(response.Body.Bytes())).Decode(&payload))
	assert.Equal(t, ResponseCodeParamsCheckError, payload.Code)
}
