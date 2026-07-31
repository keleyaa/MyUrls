package main

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
)

type failingSyncer struct{ err error }

func (s failingSyncer) Sync() error { return s.err }

type failingCloser struct {
	err    error
	closed bool
}

func (c *failingCloser) Close() error {
	c.closed = true
	return c.err
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
	writer := &failingCloser{err: closeErr}

	err := closeRequestLog(failingSyncer{err: syncErr}, writer)

	assert.ErrorIs(t, err, syncErr)
	assert.ErrorIs(t, err, closeErr)
	assert.True(t, writer.closed)
}
