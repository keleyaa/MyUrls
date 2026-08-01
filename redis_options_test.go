package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestBuildRedisOptionsPreservesLegacyConfiguration(t *testing.T) {
	options, err := BuildRedisOptions(Config{RedisAddr: "redis.internal:6379", RedisPassword: "legacy-secret"})
	require.NoError(t, err)
	assert.Equal(t, "redis.internal:6379", options.Addr)
	assert.Equal(t, "legacy-secret", options.Password)
	assert.Zero(t, options.DB)
	assert.Nil(t, options.TLSConfig)
}

func TestBuildRedisOptionsParsesRedisURLs(t *testing.T) {
	tests := []struct {
		name     string
		redisURL string
		wantTLS  bool
		wantDB   int
		wantUser string
		wantPass string
		wantAddr string
	}{
		{"plain", "redis://app:secret@cache.internal:6380/2", false, 2, "app", "secret", "cache.internal:6380"},
		{"tls", "rediss://app:secret@cache.internal:6380/3", true, 3, "app", "secret", "cache.internal:6380"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			options, err := BuildRedisOptions(Config{RedisURL: tt.redisURL, RedisAddr: "ignored:6379", RedisPassword: "ignored"})
			require.NoError(t, err)
			assert.Equal(t, tt.wantAddr, options.Addr)
			assert.Equal(t, tt.wantUser, options.Username)
			assert.Equal(t, tt.wantPass, options.Password)
			assert.Equal(t, tt.wantDB, options.DB)
			if tt.wantTLS {
				assert.NotNil(t, options.TLSConfig)
			} else {
				assert.Nil(t, options.TLSConfig)
			}
		})
	}
}

func TestBuildRedisOptionsRejectsInvalidURLWithoutLeakingIt(t *testing.T) {
	secretURL := "http://user:super-secret@cache.internal:6379/0"
	for _, redisURL := range []string{secretURL, "redis:///0", "redis://cache.internal:6379/16"} {
		_, err := BuildRedisOptions(Config{RedisURL: redisURL})
		require.EqualError(t, err, "invalid Redis URL")
		assert.NotContains(t, err.Error(), redisURL)
		assert.NotContains(t, err.Error(), "super-secret")
	}
}
