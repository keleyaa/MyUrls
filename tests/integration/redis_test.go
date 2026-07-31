//go:build integration

package integration

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const integrationTTL = 2 * time.Minute

func newRedisClient(t *testing.T) *redis.Client {
	t.Helper()

	addr := os.Getenv("MYURLS_REDIS_CONN")
	if addr == "" {
		t.Skip("MYURLS_REDIS_CONN is not set; skipping real Redis integration test")
	}

	client := redis.NewClient(&redis.Options{
		Addr:         addr,
		Password:     os.Getenv("MYURLS_REDIS_PASSWORD"),
		DialTimeout:  3 * time.Second,
		ReadTimeout:  3 * time.Second,
		WriteTimeout: 3 * time.Second,
	})
	t.Cleanup(func() {
		require.NoError(t, client.Close())
	})
	return client
}

func uniqueRedisKey(t *testing.T) string {
	t.Helper()

	randomBytes := make([]byte, 8)
	_, err := rand.Read(randomBytes)
	require.NoError(t, err)
	name := strings.NewReplacer("/", "-", " ", "-").Replace(t.Name())
	return "myurls-integration:" + name + ":" + hex.EncodeToString(randomBytes)
}

func cleanupRedisKey(t *testing.T, client *redis.Client, key string) {
	t.Helper()
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		require.NoError(t, client.Del(ctx, key).Err())
	})
}

func TestRedisPing(t *testing.T) {
	client := newRedisClient(t)
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
	defer cancel()

	pong, err := client.Ping(ctx).Result()
	require.NoError(t, err)
	require.Equal(t, "PONG", pong)
}

func TestRedisSetNXAndTTL(t *testing.T) {
	client := newRedisClient(t)
	key := uniqueRedisKey(t)
	cleanupRedisKey(t, client, key)
	ctx, cancel := context.WithTimeout(t.Context(), 5*time.Second)
	defer cancel()

	created, err := client.SetNX(ctx, key, "https://example.com/first", integrationTTL).Result()
	require.NoError(t, err)
	assert.True(t, created)

	created, err = client.SetNX(ctx, key, "https://example.com/second", integrationTTL).Result()
	require.NoError(t, err)
	assert.False(t, created)

	ttl, err := client.TTL(ctx, key).Result()
	require.NoError(t, err)
	assert.Greater(t, ttl, time.Duration(0))
	assert.LessOrEqual(t, ttl, integrationTTL)
}

func TestRedisConcurrentSetNXAllowsExactlyOneWinner(t *testing.T) {
	client := newRedisClient(t)
	key := uniqueRedisKey(t)
	cleanupRedisKey(t, client, key)
	ctx, cancel := context.WithTimeout(t.Context(), 10*time.Second)
	defer cancel()

	const workers = 100
	start := make(chan struct{})
	results := make(chan bool, workers)
	errors := make(chan error, workers)
	var ready sync.WaitGroup
	var done sync.WaitGroup
	ready.Add(workers)
	done.Add(workers)

	for i := 0; i < workers; i++ {
		go func() {
			defer done.Done()
			ready.Done()
			<-start
			created, err := client.SetNX(ctx, key, "https://example.com/concurrent", integrationTTL).Result()
			if err != nil {
				errors <- err
				return
			}
			results <- created
		}()
	}

	ready.Wait()
	close(start)
	done.Wait()
	close(results)
	close(errors)

	for err := range errors {
		require.NoError(t, err)
	}
	winners := 0
	for created := range results {
		if created {
			winners++
		}
	}
	assert.Equal(t, 1, winners)
}
