package main

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
	"go.uber.org/zap/zaptest/observer"
)

func TestCreateLogPathReturnsWorkingDirectoryError(t *testing.T) {
	originalDirectory, err := os.Getwd()
	assert.NoError(t, err)
	t.Cleanup(func() {
		assert.NoError(t, os.Chdir(originalDirectory))
	})

	parent := t.TempDir()
	removedDirectory := filepath.Join(parent, "removed")
	assert.NoError(t, os.Mkdir(removedDirectory, 0o755))
	assert.NoError(t, os.Chdir(removedDirectory))
	assert.NoError(t, os.Remove(removedDirectory))

	assert.Error(t, createLogPath())
}

func TestCreateLogPathTightensExistingDirectoryPermissions(t *testing.T) {
	originalDirectory, err := os.Getwd()
	assert.NoError(t, err)
	t.Cleanup(func() {
		assert.NoError(t, os.Chdir(originalDirectory))
	})
	temporaryDirectory := t.TempDir()
	assert.NoError(t, os.Chdir(temporaryDirectory))
	logPath := filepath.Join(temporaryDirectory, "logs")
	assert.NoError(t, os.Mkdir(logPath, 0o755))
	assert.NoError(t, os.Chmod(logPath, 0o777))

	assert.NoError(t, createLogPath())
	info, err := os.Stat(logPath)
	assert.NoError(t, err)
	assert.Equal(t, os.FileMode(0o755), info.Mode().Perm())
}

func TestRequestLoggerIsSharedAndClosable(t *testing.T) {
	first := initGinLogger()
	second := initGinLogger()
	assert.Same(t, first, second)
	assert.NoError(t, CloseRequestLogger())
	assert.NoError(t, CloseRequestLogger())
}

func TestCloseRequestLoggerIsNoopWhenUninitialized(t *testing.T) {
	requestLoggerMu.Lock()
	requestLogger = nil
	requestLoggerWriter = nil
	requestLoggerMu.Unlock()

	assert.NoError(t, CloseRequestLogger())
}

func TestCloseRequestLogJoinsSyncAndCloseErrors(t *testing.T) {
	syncErr := errors.New("sync failed")
	closeErr := errors.New("close failed")
	closed := false

	err := closeRequestLog(func() error { return syncErr }, func() error {
		closed = true
		return closeErr
	})

	assert.ErrorIs(t, err, syncErr)
	assert.ErrorIs(t, err, closeErr)
	assert.True(t, closed)
}

func TestCloseRequestLogHandlesNilAndCloseOnlyFunctions(t *testing.T) {
	assert.NoError(t, closeRequestLog(nil, nil))

	closed := false
	assert.NoError(t, closeRequestLog(nil, func() error {
		closed = true
		return nil
	}))
	assert.True(t, closed)

	requestLoggerMu.Lock()
	var typedNilLogger *zap.Logger
	requestLogger = typedNilLogger
	requestLoggerWriter = nil
	requestLoggerMu.Unlock()
	assert.NoError(t, CloseRequestLogger())
}

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

	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})
	router.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/private-short-code", nil))

	assert.NotContains(t, defaultOutput.String(), "private-short-code")
}
