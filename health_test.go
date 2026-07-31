package main

import (
	"context"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"strconv"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHealthHandlerReturnsOKWhenDependencyResponds(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.GET("/healthz", HealthHandler(func(context.Context) error { return nil }))

	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	assert.Equal(t, http.StatusOK, response.Code)
	assert.JSONEq(t, `{"status":"ok"}`, response.Body.String())
}

func TestHealthHandlerReturnsRedactedServiceUnavailableWhenDependencyFails(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.GET("/healthz", HealthHandler(func(context.Context) error {
		return errors.New("dial tcp redis.internal:6379: connection refused")
	}))

	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	assert.Equal(t, http.StatusServiceUnavailable, response.Code)
	assert.JSONEq(t, `{"status":"unavailable"}`, response.Body.String())
	assert.NotContains(t, response.Body.String(), "redis.internal")
	assert.NotContains(t, response.Body.String(), "connection refused")
}

func TestHealthHandlerReturnsServiceUnavailableWithoutPingDependency(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.GET("/healthz", HealthHandler(nil))

	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/healthz", nil))

	assert.Equal(t, http.StatusServiceUnavailable, response.Code)
	assert.JSONEq(t, `{"status":"unavailable"}`, response.Body.String())
}

func TestRunHealthcheckAcceptsOnlyOK(t *testing.T) {
	t.Run("ok", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "/healthz", r.URL.Path)
			w.WriteHeader(http.StatusOK)
		}))
		defer server.Close()

		port := server.Listener.Addr().(*net.TCPAddr).Port
		require.NoError(t, RunHealthcheck(strconv.Itoa(port)))
	})

	t.Run("non OK", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusServiceUnavailable)
		}))
		defer server.Close()

		port := server.Listener.Addr().(*net.TCPAddr).Port
		assert.Error(t, RunHealthcheck(strconv.Itoa(port)))
	})

	t.Run("connection error", func(t *testing.T) {
		listener, err := net.Listen("tcp", "127.0.0.1:0")
		require.NoError(t, err)
		port := listener.Addr().(*net.TCPAddr).Port
		require.NoError(t, listener.Close())

		assert.Error(t, RunHealthcheck(strconv.Itoa(port)))
	})
}
