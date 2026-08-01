package main

import (
	"errors"
	"net/url"

	"github.com/redis/go-redis/v9"
)

var errInvalidRedisURL = errors.New("invalid Redis URL")

// BuildRedisOptions keeps the legacy host/password contract unless a managed
// Redis URL is explicitly configured.
func BuildRedisOptions(cfg Config) (*redis.Options, error) {
	if cfg.RedisURL == "" {
		return &redis.Options{Addr: cfg.RedisAddr, Password: cfg.RedisPassword, DB: 0}, nil
	}

	parsed, err := url.Parse(cfg.RedisURL)
	if err != nil || (parsed.Scheme != "redis" && parsed.Scheme != "rediss") || parsed.Host == "" {
		return nil, errInvalidRedisURL
	}
	options, err := redis.ParseURL(cfg.RedisURL)
	if err != nil || options.Addr == "" || options.DB < 0 || options.DB > 15 {
		return nil, errInvalidRedisURL
	}
	return options, nil
}
