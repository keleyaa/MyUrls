// FILEPATH: /root/CareyWang/MyUrls/logic_test.go

package main

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestLongToShortAndShortToLong(t *testing.T) {
	ctx := context.Background()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	shortKey := "testKey"
	longURL := "https://example.com"

	err := LongToShort(ctx, &LongToShortOptions{
		ShortKey:   shortKey,
		URL:        longURL,
		expiration: 60 * time.Second,
	})
	assert.NoError(t, err)

	resultLongURL := ShortToLong(ctx, shortKey)
	assert.Equal(t, longURL, resultLongURL)
}
