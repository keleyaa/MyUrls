package main

import (
	"context"
	"time"

	"github.com/redis/go-redis/v9"
)

var RedisClient *redis.Client

// initRedisClient is a function that takes a pointer to a RedisOptions struct and returns a pointer to a Redis client.
func initRedisClient(options *redis.Options) {
	RedisClient = redis.NewClient(options)
}

func GetRedisClient() *redis.Client {
	return RedisClient
}

// StoreShortURL stores a URL only if no mapping already exists for key.
func StoreShortURL(ctx context.Context, key, value string, ttl time.Duration) (bool, error) {
	return GetRedisClient().SetNX(ctx, key, value, ttl).Result()
}

// LoadLongURL returns the URL stored for key.
func LoadLongURL(ctx context.Context, key string) (string, error) {
	return GetRedisClient().Get(ctx, key).Result()
}
