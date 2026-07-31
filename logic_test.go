// FILEPATH: /root/CareyWang/MyUrls/logic_test.go

package main

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
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

func TestCreateShortURLAtomicallyClaimsRequestedKey(t *testing.T) {
	ctx := context.Background()
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	const requestedKey = "shared-key"
	const workers = 100

	type result struct {
		url string
		err error
	}
	results := make(chan result, workers)
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			url := "https://example.com/" + string(rune('a'+i%26))
			_, err := CreateShortURL(ctx, requestedKey, url)
			results <- result{url: url, err: err}
		}(i)
	}
	wg.Wait()
	close(results)

	var winner string
	successes := 0
	conflicts := 0
	for result := range results {
		switch {
		case result.err == nil:
			successes++
			winner = result.url
		case errors.Is(result.err, ErrShortKeyExists):
			conflicts++
		default:
			t.Fatalf("CreateShortURL returned unexpected error: %v", result.err)
		}
	}

	assert.Equal(t, 1, successes)
	assert.Equal(t, workers-1, conflicts)
	stored, err := ResolveShortURL(ctx, requestedKey)
	require.NoError(t, err)
	assert.Equal(t, winner, stored)
}
