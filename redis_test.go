package main

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
)

func TestRedisOperationsReturnStableErrorWithoutClient(t *testing.T) {
	resetRedisClient(t)

	_, err := StoreShortURL(t.Context(), "key", "https://example.com", time.Hour)
	assert.ErrorIs(t, err, ErrRedisClientUnavailable)

	_, err = LoadLongURL(t.Context(), "key")
	assert.ErrorIs(t, err, ErrRedisClientUnavailable)
}

func TestCloseRedisClientIsSafeDuringConcurrentCalls(t *testing.T) {
	resetRedisClient(t)
	initRedisClient(newTestRedisOptions(t))

	var group sync.WaitGroup
	for range 32 {
		group.Add(1)
		go func() {
			defer group.Done()
			assert.NoError(t, CloseRedisClient())
		}()
	}
	group.Wait()

	assert.Nil(t, GetRedisClient())
	assert.True(t, errors.Is(func() error {
		_, err := LoadLongURL(t.Context(), "key")
		return err
	}(), ErrRedisClientUnavailable))
}

func newTestRedisOptions(t testing.TB) *redis.Options {
	t.Helper()

	server, err := miniredis.Run()
	if err != nil {
		t.Fatalf("start test Redis: %v", err)
	}
	t.Cleanup(server.Close)

	return &redis.Options{Addr: server.Addr()}
}

func resetRedisClient(t testing.TB) {
	t.Helper()

	redisClientMu.Lock()
	previous := redisClient
	redisClient = nil
	redisClientMu.Unlock()
	t.Cleanup(func() {
		redisClientMu.Lock()
		current := redisClient
		redisClient = previous
		redisClientMu.Unlock()
		if current != nil && current != previous {
			_ = current.Close()
		}
	})
}

func TestGetRedisClient(t *testing.T) {
	resetRedisClient(t)

	client := GetRedisClient()
	assert.Nil(t, client)

	initRedisClient(newTestRedisOptions(t))
	client = GetRedisClient()
	assert.NotNil(t, client)

	// Test redis exec commands and response
	ctx := context.Background()
	rs := client.Ping(ctx)
	assert.Nil(t, rs.Err())
	assert.Equal(t, "PONG", rs.Val())

	rsCmd := GetRedisClient().Do(ctx, "dbsize")
	assert.Nil(t, rsCmd.Err())
}

func BenchmarkGetRedisClient(b *testing.B) {
	resetRedisClient(b)
	initRedisClient(newTestRedisOptions(b))

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		GetRedisClient().Get(context.Background(), "key")
	}
}
