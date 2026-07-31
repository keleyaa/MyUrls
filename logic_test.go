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

func TestLegacyRedisHelpersDoNotPanicWithoutClient(t *testing.T) {
	resetRedisClient(t)

	assert.Empty(t, ShortToLong(t.Context(), "key"))
	assert.ErrorIs(t, LongToShort(t.Context(), &LongToShortOptions{ShortKey: "key", URL: "https://example.com", expiration: time.Hour}), ErrRedisClientUnavailable)
	assert.ErrorIs(t, Renew(t.Context(), "key", time.Hour), ErrRedisClientUnavailable)
	_, err := CheckRedisKeyIfExist(t.Context(), "key")
	assert.ErrorIs(t, err, ErrRedisClientUnavailable)
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
	ready := make(chan struct{}, workers)
	start := make(chan struct{})
	var wg sync.WaitGroup
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			ready <- struct{}{}
			<-start
			url := "https://example.com/" + string(rune('a'+i%26))
			_, err := CreateShortURL(ctx, requestedKey, url)
			results <- result{url: url, err: err}
		}(i)
	}
	for range workers {
		<-ready
	}
	close(start)
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

func TestCreateShortURLRetriesGeneratedKeysAfterCollisions(t *testing.T) {
	for _, tt := range []struct {
		name string
		keys []string
	}{
		{name: "one collision", keys: []string{"taken", "available"}},
		{name: "multiple collisions", keys: []string{"taken-one", "taken-two", "taken-three", "available"}},
	} {
		t.Run(tt.name, func(t *testing.T) {
			resetRedisClient(t)
			initRedisClient(newTestRedisOptions(t))
			for _, key := range tt.keys[:len(tt.keys)-1] {
				created, err := StoreShortURL(t.Context(), key, "https://existing.example", time.Hour)
				require.NoError(t, err)
				require.True(t, created)
			}

			attempts := 0
			key, err := createShortURL(t.Context(), "", "https://new.example", func(int) (string, error) {
				key := tt.keys[attempts]
				attempts++
				return key, nil
			})

			require.NoError(t, err)
			assert.Equal(t, tt.keys[len(tt.keys)-1], key)
			assert.Equal(t, len(tt.keys), attempts)
			stored, err := ResolveShortURL(t.Context(), key)
			require.NoError(t, err)
			assert.Equal(t, "https://new.example", stored)
		})
	}
}

func TestCreateShortURLReturnsExhaustedAfterFiveGeneratedCollisions(t *testing.T) {
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	keys := []string{"taken-one", "taken-two", "taken-three", "taken-four", "taken-five"}
	for _, key := range keys {
		created, err := StoreShortURL(t.Context(), key, "https://existing.example", time.Hour)
		require.NoError(t, err)
		require.True(t, created)
	}

	attempts := 0
	key, err := createShortURL(t.Context(), "", "https://new.example", func(int) (string, error) {
		key := keys[attempts]
		attempts++
		return key, nil
	})

	assert.Empty(t, key)
	assert.ErrorIs(t, err, ErrShortKeyExhausted)
	assert.Equal(t, 5, attempts)
}
