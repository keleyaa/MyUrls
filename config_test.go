package main

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func mapLookup(values map[string]string) LookupEnv {
	return func(key string) (string, bool) {
		value, ok := values[key]
		return value, ok
	}
}

func TestLoadConfigDefaults(t *testing.T) {
	cfg, err := LoadConfig(nil, mapLookup(nil))
	require.NoError(t, err)

	assert.Equal(t, "8080", cfg.Port)
	assert.Equal(t, "localhost:8080", cfg.Domain)
	assert.Equal(t, "https", cfg.Proto)
	assert.Nil(t, cfg.BaseURL)
	assert.Equal(t, "localhost:6379", cfg.RedisAddr)
	assert.Empty(t, cfg.RedisURL)
	assert.Empty(t, cfg.RedisPassword)
	assert.Empty(t, cfg.APIToken)
	assert.Zero(t, cfg.RateLimitRPS)
	assert.Equal(t, 10, cfg.RateLimitBurst)
	assert.Equal(t, 16_384, cfg.MaxBodyBytes)
	assert.Equal(t, 5*time.Second, cfg.ReadHeaderTimeout)
	assert.Equal(t, 10*time.Second, cfg.ReadTimeout)
	assert.Equal(t, 10*time.Second, cfg.WriteTimeout)
	assert.Equal(t, 60*time.Second, cfg.IdleTimeout)
	assert.Equal(t, 10*time.Second, cfg.ShutdownTimeout)
	assert.False(t, cfg.Healthcheck)
}

func TestLoadConfigLegacyFlags(t *testing.T) {
	cfg, err := LoadConfig([]string{
		"-port", "8081",
		"-domain", "s.example.com",
		"-proto", "http",
		"-conn", "redis:6379",
		"-password", "secret",
		"-healthcheck",
	}, mapLookup(nil))
	require.NoError(t, err)

	assert.Equal(t, "8081", cfg.Port)
	assert.Equal(t, "s.example.com", cfg.Domain)
	assert.Equal(t, "http", cfg.Proto)
	assert.Equal(t, "redis:6379", cfg.RedisAddr)
	assert.Equal(t, "secret", cfg.RedisPassword)
	assert.True(t, cfg.Healthcheck)
}

func TestLoadConfigEnvironmentOverridesFlags(t *testing.T) {
	env := map[string]string{
		"MYURLS_PORT":                "9090",
		"MYURLS_DOMAIN":              "links.example.com",
		"MYURLS_PROTO":               "http",
		"MYURLS_BASE_URL":            "https://public.example/links/",
		"MYURLS_REDIS_CONN":          "cache:6379",
		"MYURLS_REDIS_URL":           "rediss://app:secret@managed.internal:6380/1",
		"MYURLS_REDIS_PASSWORD":      "env-secret",
		"MYURLS_API_TOKEN":           "token",
		"MYURLS_RATE_LIMIT_RPS":      "2.5",
		"MYURLS_RATE_LIMIT_BURST":    "4",
		"MYURLS_MAX_BODY_BYTES":      "32768",
		"MYURLS_READ_HEADER_TIMEOUT": "6s",
		"MYURLS_READ_TIMEOUT":        "11s",
		"MYURLS_WRITE_TIMEOUT":       "12s",
		"MYURLS_IDLE_TIMEOUT":        "70s",
		"MYURLS_SHUTDOWN_TIMEOUT":    "13s",
	}

	cfg, err := LoadConfig([]string{"-port", "8081"}, mapLookup(env))
	require.NoError(t, err)

	assert.Equal(t, "9090", cfg.Port)
	assert.Equal(t, "links.example.com", cfg.Domain)
	assert.Equal(t, "http", cfg.Proto)
	require.NotNil(t, cfg.BaseURL)
	assert.Equal(t, "https://public.example/links/short-key", cfg.ShortURL("short-key"))
	assert.Equal(t, "cache:6379", cfg.RedisAddr)
	assert.Equal(t, "rediss://app:secret@managed.internal:6380/1", cfg.RedisURL)
	assert.Equal(t, "env-secret", cfg.RedisPassword)
	assert.Equal(t, "token", cfg.APIToken)
	assert.Equal(t, 2.5, cfg.RateLimitRPS)
	assert.Equal(t, 4, cfg.RateLimitBurst)
	assert.Equal(t, 32768, cfg.MaxBodyBytes)
	assert.Equal(t, 6*time.Second, cfg.ReadHeaderTimeout)
	assert.Equal(t, 11*time.Second, cfg.ReadTimeout)
	assert.Equal(t, 12*time.Second, cfg.WriteTimeout)
	assert.Equal(t, 70*time.Second, cfg.IdleTimeout)
	assert.Equal(t, 13*time.Second, cfg.ShutdownTimeout)
}

func TestLoadConfigAllowsEmptyBaseURL(t *testing.T) {
	cfg, err := LoadConfig(nil, mapLookup(map[string]string{
		"MYURLS_BASE_URL": "",
	}))
	require.NoError(t, err)
	assert.Nil(t, cfg.BaseURL)
}

func TestLoadConfigRejectsInvalidValues(t *testing.T) {
	tests := []struct {
		name string
		env  map[string]string
	}{
		{"negative rate", map[string]string{"MYURLS_RATE_LIMIT_RPS": "-1"}},
		{"zero burst when enabled", map[string]string{"MYURLS_RATE_LIMIT_RPS": "1", "MYURLS_RATE_LIMIT_BURST": "0"}},
		{"small body", map[string]string{"MYURLS_MAX_BODY_BYTES": "1023"}},
		{"invalid duration", map[string]string{"MYURLS_READ_TIMEOUT": "soon"}},
		{"zero timeout", map[string]string{"MYURLS_IDLE_TIMEOUT": "0s"}},
		{"invalid base URL", map[string]string{"MYURLS_BASE_URL": "ftp://public.example"}},
		{"base URL credentials", map[string]string{"MYURLS_BASE_URL": "https://user:secret@public.example"}},
		{"base URL query", map[string]string{"MYURLS_BASE_URL": "https://public.example/?key=secret"}},
		{"base URL fragment", map[string]string{"MYURLS_BASE_URL": "https://public.example/#secret"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := LoadConfig(nil, mapLookup(tt.env))
			require.Error(t, err)
		})
	}
}

func TestLoadConfigDoesNotEchoInvalidEnvironmentValues(t *testing.T) {
	const secret = "super-secret-value"
	for name, env := range map[string]map[string]string{
		"float":    {"MYURLS_RATE_LIMIT_RPS": secret},
		"integer":  {"MYURLS_RATE_LIMIT_BURST": secret},
		"duration": {"MYURLS_READ_TIMEOUT": secret},
	} {
		t.Run(name, func(t *testing.T) {
			_, err := LoadConfig(nil, mapLookup(env))
			require.Error(t, err)
			assert.NotContains(t, err.Error(), secret)
		})
	}
}

func TestLoadConfigRejectsInvalidBaseURLWithoutLeakingValue(t *testing.T) {
	_, err := LoadConfig(nil, mapLookup(map[string]string{
		"MYURLS_BASE_URL": "https://user:super-secret@public.example",
	}))
	require.EqualError(t, err, "invalid MYURLS_BASE_URL")
	assert.NotContains(t, err.Error(), "super-secret")
}

func TestLoadConfigRejectsInvalidRedisURLWithoutLeakingCredentials(t *testing.T) {
	_, err := LoadConfig(nil, mapLookup(map[string]string{
		"MYURLS_REDIS_URL": "http://user:super-secret@cache.internal:6379/0",
	}))
	require.EqualError(t, err, "invalid Redis URL")
	assert.NotContains(t, err.Error(), "super-secret")
}
