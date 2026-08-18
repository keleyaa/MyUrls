package main

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/redis/go-redis/v9"
)

var ErrRedisClientUnavailable = errors.New("redis client unavailable")

// Store owns one Redis client for one application lifetime.
type Store struct {
	client *redis.Client
	ping   func(context.Context) error
	close  func() error
	once   sync.Once
	err    error
}

func NewStore(options *redis.Options) *Store {
	client := redis.NewClient(options)
	return &Store{
		client: client,
		ping:   func(ctx context.Context) error { return client.Ping(ctx).Err() },
		close:  client.Close,
	}
}

func (s *Store) Close() error {
	if s == nil || s.close == nil {
		return nil
	}
	s.once.Do(func() {
		s.err = s.close()
	})
	return s.err
}

func (s *Store) Ping(ctx context.Context) error {
	if s == nil || s.ping == nil {
		return ErrRedisClientUnavailable
	}
	return s.ping(ctx)
}

// StoreShortURL atomically stores value only when key has no existing mapping.
func (s *Store) StoreShortURL(ctx context.Context, key, value string, ttl time.Duration) (bool, error) {
	if s == nil || s.client == nil {
		return false, ErrRedisClientUnavailable
	}
	return s.client.SetNX(ctx, key, value, ttl).Result()
}

// LoadLongURL returns the stored target URL. redis.Nil is returned unchanged.
func (s *Store) LoadLongURL(ctx context.Context, key string) (string, error) {
	if s == nil || s.client == nil {
		return "", ErrRedisClientUnavailable
	}
	return s.client.Get(ctx, key).Result()
}
