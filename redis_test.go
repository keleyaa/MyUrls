package main

import (
	"context"
	"testing"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
)

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

	previous := RedisClient
	RedisClient = nil
	t.Cleanup(func() {
		if RedisClient != nil && RedisClient != previous {
			_ = RedisClient.Close()
		}
		RedisClient = previous
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
