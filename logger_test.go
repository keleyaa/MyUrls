package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestRequestLoggerIsSharedAndClosable(t *testing.T) {
	first := initGinLogger()
	second := initGinLogger()
	assert.Same(t, first, second)
	assert.NoError(t, CloseRequestLogger())
	assert.NoError(t, CloseRequestLogger())
}
