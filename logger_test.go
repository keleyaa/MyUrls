package main

import (
	"bytes"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
	"go.uber.org/zap/zaptest/observer"
)

func TestFormatChinaTimeUsesUTCPlusEight(t *testing.T) {
	instant := time.Date(2026, time.August, 5, 0, 30, 0, 0, time.UTC)

	assert.Equal(t, "2026-08-05T08:30:00+08:00", formatChinaTime(instant))
}

func TestZapEncoderUsesUTCPlusEight(t *testing.T) {
	var output bytes.Buffer
	core := zap.New(zapcore.NewCore(getEncoder(), zapcore.AddSync(&output), zap.DebugLevel))

	core.Info("timezone-check")

	assert.Contains(t, output.String(), "+08:00")
}

func TestPrivacySafeRouteNeverReturnsRawUnmatchedPath(t *testing.T) {
	assert.Equal(t, "/:shortKey", privacySafeRoute("/:shortKey"))
	assert.Equal(t, "/short", privacySafeRoute("/short"))
	assert.Equal(t, "unmatched", privacySafeRoute(""))
}

func TestServiceLoggerUsesRouteTemplateWithoutClientIdentifiers(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core, observed := observer.New(zap.InfoLevel)
	router := gin.New()
	router.Use(serviceLoggerMiddleware(zap.New(core)))
	router.GET("/:shortKey", func(c *gin.Context) { c.Status(http.StatusTemporaryRedirect) })

	request := httptest.NewRequest(http.MethodGet, "/private-short-code", nil)
	request.Header.Set("User-Agent", "sensitive-client")
	request.RemoteAddr = "203.0.113.10:12345"
	router.ServeHTTP(httptest.NewRecorder(), request)

	entries := observed.All()
	if assert.Len(t, entries, 1) {
		fields := entries[0].ContextMap()
		assert.Equal(t, "/:shortKey", fields["route"])
		assert.NotContains(t, fields, "path")
		assert.NotContains(t, fields, "ip")
		assert.NotContains(t, fields, "user-agent")
		assert.NotContains(t, strings.ToLower(entries[0].Message), "private-short-code")
	}
}

func TestServiceLoggerSuppressesSuccessfulHealthChecks(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core, observed := observer.New(zap.InfoLevel)
	router := gin.New()
	router.Use(serviceLoggerMiddleware(zap.New(core)))
	router.GET("/healthz", func(c *gin.Context) { c.Status(http.StatusOK) })

	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/healthz", nil))

	assert.Empty(t, observed.All())
}

func TestServiceLoggerKeepsFailedHealthChecks(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core, observed := observer.New(zap.InfoLevel)
	router := gin.New()
	router.Use(serviceLoggerMiddleware(zap.New(core)))
	router.GET("/healthz", func(c *gin.Context) { c.Status(http.StatusServiceUnavailable) })

	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/healthz", nil))

	entries := observed.All()
	if assert.Len(t, entries, 1) {
		assert.Equal(t, "/healthz", entries[0].ContextMap()["route"])
		assert.Equal(t, int64(http.StatusServiceUnavailable), entries[0].ContextMap()["status"])
	}
}

func TestNewRouterDoesNotInstallGinDefaultLogger(t *testing.T) {
	gin.SetMode(gin.TestMode)
	var defaultOutput bytes.Buffer
	originalWriter := gin.DefaultWriter
	gin.DefaultWriter = &defaultOutput
	t.Cleanup(func() { gin.DefaultWriter = originalWriter })

	store, _ := newTestStore(t)
	router := NewApp(defaultConfig(), store).Router()
	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/private-short-code", nil))

	assert.NotContains(t, defaultOutput.String(), "private-short-code")
}
