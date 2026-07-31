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
		})
	}

	keys, err := GetRedisClient().Keys(t.Context(), "*").Result()
	require.NoError(t, err)
	assert.NotContains(t, keys, "danger")
	assert.NotContains(t, keys, "healthz")
}
