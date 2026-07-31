package main

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"go.uber.org/zap"
)

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
