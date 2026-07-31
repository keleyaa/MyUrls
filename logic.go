package main

import (
	"context"
	"errors"
	"time"
)

var (
	ErrShortKeyExists    = errors.New("short key already exists")
	ErrShortKeyExhausted = errors.New("could not generate an available short key")
)

const defaultTTL = time.Hour * 24 * 365 // 默认过期时间，1年

// ShortToLong gets the long URL from a short URL
func ShortToLong(ctx context.Context, shortKey string) string {
	rc := GetRedisClient()
	if rc == nil {
		return ""
	}
	return rc.Get(ctx, shortKey).Val()
}

// LongToShortOptions are the options for the LongToShort function
type LongToShortOptions struct {
	ShortKey   string
	URL        string
	expiration time.Duration
}

// LongToShort creates a short URL from a long URL
func LongToShort(ctx context.Context, options *LongToShortOptions) error {
	rc := GetRedisClient()
	if rc == nil {
		return ErrRedisClientUnavailable
	}
	return rc.SetEx(ctx, options.ShortKey, options.URL, options.expiration).Err()
}

// CreateShortURL stores longURL under requestedKey, or generates a key when none is requested.
func CreateShortURL(ctx context.Context, requestedKey, longURL string) (string, error) {
	return createShortURL(ctx, requestedKey, longURL, GenerateRandomString)
}

type shortKeyGenerator func(int) (string, error)

func createShortURL(ctx context.Context, requestedKey, longURL string, generateShortKey shortKeyGenerator) (string, error) {
	if requestedKey != "" {
		created, err := StoreShortURL(ctx, requestedKey, longURL, defaultTTL)
		if err != nil {
			return "", err
		}
		if !created {
			return "", ErrShortKeyExists
		}
		return requestedKey, nil
	}

	for range 5 {
		shortKey, err := generateShortKey(defaultShortKeyLength)
		if err != nil {
			return "", err
		}

		created, err := StoreShortURL(ctx, shortKey, longURL, defaultTTL)
		if err != nil {
			return "", err
		}
		if created {
			return shortKey, nil
		}
	}

	return "", ErrShortKeyExhausted
}

// ResolveShortURL returns the long URL for shortKey.
func ResolveShortURL(ctx context.Context, shortKey string) (string, error) {
	return LoadLongURL(ctx, shortKey)
}

// Renew updates the expiration time of a short URL
func Renew(ctx context.Context, shortKey string, expiration time.Duration) error {
	rc := GetRedisClient()
	if rc == nil {
		return ErrRedisClientUnavailable
	}

	rs := rc.TTL(ctx, shortKey)
	if rs.Err() != nil {
		return rs.Err()
	}

	ttl := rs.Val()
	if ttl < 0 {
		return nil
	}

	return rc.Expire(ctx, shortKey, ttl+expiration).Err()
}

func CheckRedisKeyIfExist(ctx context.Context, key string) (bool, error) {
	rc := GetRedisClient()
	if rc == nil {
		return false, ErrRedisClientUnavailable
	}
	rs := rc.Exists(ctx, key)
	if rs.Err() != nil {
		return false, rs.Err()
	}

	return rs.Val() > 0, nil
}
