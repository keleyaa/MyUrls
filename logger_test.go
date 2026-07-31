package main

import (
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"testing"

	"github.com/stretchr/testify/assert"
	"go.uber.org/zap"
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

func TestCreateLogPathUsesRestrictedDirectoryPermissions(t *testing.T) {
	originalDirectory, err := os.Getwd()
	assert.NoError(t, err)
	t.Cleanup(func() {
		assert.NoError(t, os.Chdir(originalDirectory))
	})
	assert.NoError(t, os.Chdir(t.TempDir()))

	originalUmask := syscall.Umask(0)
	t.Cleanup(func() { syscall.Umask(originalUmask) })

	assert.NoError(t, createLogPath())
	info, err := os.Stat("logs")
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
