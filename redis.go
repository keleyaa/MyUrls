package main

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/redis/go-redis/v9"
)

var (
	redisClient               *redis.Client
	redisClientMu             sync.RWMutex
	ErrRedisClientUnavailable = errors.New("redis client unavailable")
)

// initRedisClient is a function that takes a pointer to a RedisOptions struct and returns a pointer to a Redis client.
func initRedisClient(options *redis.Options) {
	client := redis.NewClient(options)

	redisClientMu.Lock()
	previous := redisClient
	redisClient = client
	redisClientMu.Unlock()

	if previous != nil {
		_ = previous.Close()
	}
}

func GetRedisClient() *redis.Client {
	redisClientMu.RLock()
	defer redisClientMu.RUnlock()
	return redisClient
}

// CloseRedisClient closes the active client. It is safe to call more than once.
func CloseRedisClient() error {
	redisClientMu.Lock()
	client := redisClient
	redisClient = nil
	redisClientMu.Unlock()

	if client == nil {
		return nil
	}
	return client.Close()
}

// StoreShortURL stores a URL only if no mapping already exists for key.
func StoreShortURL(ctx context.Context, key, value string, ttl time.Duration) (bool, error) {
	client := GetRedisClient()
	if client == nil {
		return false, ErrRedisClientUnavailable
	}
	return client.SetNX(ctx, key, value, ttl).Result()
}

// LoadLongURL returns the URL stored for key.
func LoadLongURL(ctx context.Context, key string) (string, error) {
	client := GetRedisClient()
	if client == nil {
		return "", ErrRedisClientUnavailable
	}
	return client.Get(ctx, key).Result()
}
