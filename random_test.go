package main

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

type errReader struct{}

func (errReader) Read([]byte) (int, error) {
	return 0, errors.New("random source unavailable")
}

func TestGenerateRandomStringUsesAllowedCharacters(t *testing.T) {
	value, err := GenerateRandomString(7)

	require.NoError(t, err)
	assert.Len(t, value, 7)
	for _, char := range value {
		assert.Contains(t, letterBytes, string(char))
	}
}

func TestGenerateRandomStringPropagatesReaderError(t *testing.T) {
	value, err := generateRandomString(errReader{}, 7)

	assert.Empty(t, value)
	assert.EqualError(t, err, "random source unavailable")
}
