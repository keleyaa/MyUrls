package main

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"
	"github.com/stretchr/testify/assert"
)

func newTestStore(t testing.TB) (*Store, *redis.Client) {
	t.Helper()

	server, err := miniredis.Run()
	if err != nil {
		t.Fatalf("start test Redis: %v", err)
	}
	options := &redis.Options{Addr: server.Addr()}
	store := NewStore(options)
	client := redis.NewClient(options)
	t.Cleanup(func() {
		_ = client.Close()
		_ = store.Close()
		server.Close()
	})
	return store, client
}

func TestStoreOperationsReturnStableErrorWithoutClient(t *testing.T) {
	store := &Store{}

	_, err := store.StoreShortURL(t.Context(), "key", "https://example.com", time.Hour)
	assert.ErrorIs(t, err, ErrRedisClientUnavailable)

	_, err = store.LoadLongURL(t.Context(), "key")
	assert.ErrorIs(t, err, ErrRedisClientUnavailable)
}

func TestStoreCloseIsSafeDuringConcurrentCalls(t *testing.T) {
	store, _ := newTestStore(t)

	var group sync.WaitGroup
	for range 32 {
		group.Add(1)
		go func() {
			defer group.Done()
			assert.NoError(t, store.Close())
		}()
	}
	group.Wait()

	_, err := store.LoadLongURL(t.Context(), "key")
	assert.ErrorIs(t, err, redis.ErrClosed)
}

func TestStorePing(t *testing.T) {
	store, _ := newTestStore(t)

	assert.NoError(t, store.Ping(context.Background()))
}
