package main

import (
	"errors"
	"flag"
	"fmt"
	"strconv"
	"time"
)

type Config struct {
	Port           string
	Domain         string
	Proto          string
	RedisAddr      string
	RedisURL       string
	RedisPassword  string
	APIToken       string
	RateLimitRPS   float64
	RateLimitBurst int
	MaxBodyBytes   int

	ReadHeaderTimeout time.Duration
	ReadTimeout       time.Duration
	WriteTimeout      time.Duration
	IdleTimeout       time.Duration
	ShutdownTimeout   time.Duration
	Healthcheck       bool
}

type LookupEnv func(string) (string, bool)

func defaultConfig() Config {
	return Config{
		Port:              "8080",
		Domain:            "localhost:8080",
		Proto:             "https",
		RedisAddr:         "localhost:6379",
		RateLimitBurst:    10,
		MaxBodyBytes:      16_384,
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       10 * time.Second,
		WriteTimeout:      10 * time.Second,
		IdleTimeout:       60 * time.Second,
		ShutdownTimeout:   10 * time.Second,
	}
}

func LoadConfig(args []string, lookup LookupEnv) (Config, error) {
	cfg := defaultConfig()
	flags := flag.NewFlagSet("myurls", flag.ContinueOnError)
	help := false

	flags.BoolVar(&help, "h", false, "display help")
	flags.StringVar(&cfg.Port, "port", cfg.Port, "port to run the server on")
	flags.StringVar(&cfg.Domain, "domain", cfg.Domain, "domain of the server")
	flags.StringVar(&cfg.Proto, "proto", cfg.Proto, "protocol of the server")
	flags.StringVar(&cfg.RedisAddr, "conn", cfg.RedisAddr, "address of the redis server")
	flags.StringVar(&cfg.RedisPassword, "password", cfg.RedisPassword, "password of the redis server")
	flags.BoolVar(&cfg.Healthcheck, "healthcheck", false, "check service health and exit")

	if err := flags.Parse(args); err != nil {
		return Config{}, err
	}
	if help {
		flags.Usage()
		return Config{}, flag.ErrHelp
	}
	if lookup == nil {
		return Config{}, errors.New("environment lookup is required")
	}
	if err := applyEnvironment(&cfg, lookup); err != nil {
		return Config{}, err
	}
	if err := cfg.validate(); err != nil {
		return Config{}, err
	}

	return cfg, nil
}

func applyEnvironment(cfg *Config, lookup LookupEnv) error {
	setString := func(name string, target *string) {
		if value, ok := lookup(name); ok {
			*target = value
		}
	}
	setString("MYURLS_PORT", &cfg.Port)
	setString("MYURLS_DOMAIN", &cfg.Domain)
	setString("MYURLS_PROTO", &cfg.Proto)
	setString("MYURLS_REDIS_CONN", &cfg.RedisAddr)
	setString("MYURLS_REDIS_URL", &cfg.RedisURL)
	setString("MYURLS_REDIS_PASSWORD", &cfg.RedisPassword)
	setString("MYURLS_API_TOKEN", &cfg.APIToken)

	if err := applyFloat(lookup, "MYURLS_RATE_LIMIT_RPS", &cfg.RateLimitRPS); err != nil {
		return err
	}
	if err := applyInt(lookup, "MYURLS_RATE_LIMIT_BURST", &cfg.RateLimitBurst); err != nil {
		return err
	}
	if err := applyInt(lookup, "MYURLS_MAX_BODY_BYTES", &cfg.MaxBodyBytes); err != nil {
		return err
	}
	if err := applyDuration(lookup, "MYURLS_READ_HEADER_TIMEOUT", &cfg.ReadHeaderTimeout); err != nil {
		return err
	}
	if err := applyDuration(lookup, "MYURLS_READ_TIMEOUT", &cfg.ReadTimeout); err != nil {
		return err
	}
	if err := applyDuration(lookup, "MYURLS_WRITE_TIMEOUT", &cfg.WriteTimeout); err != nil {
		return err
	}
	if err := applyDuration(lookup, "MYURLS_IDLE_TIMEOUT", &cfg.IdleTimeout); err != nil {
		return err
	}
	return applyDuration(lookup, "MYURLS_SHUTDOWN_TIMEOUT", &cfg.ShutdownTimeout)
}

func applyFloat(lookup LookupEnv, name string, target *float64) error {
	value, ok := lookup(name)
	if !ok {
		return nil
	}
	parsed, err := strconv.ParseFloat(value, 64)
	if err != nil {
		return fmt.Errorf("parse %s: %w", name, err)
	}
	*target = parsed
	return nil
}

func applyInt(lookup LookupEnv, name string, target *int) error {
	value, ok := lookup(name)
	if !ok {
		return nil
	}
	parsed, err := strconv.Atoi(value)
	if err != nil {
		return fmt.Errorf("parse %s: %w", name, err)
	}
	*target = parsed
	return nil
}

func applyDuration(lookup LookupEnv, name string, target *time.Duration) error {
	value, ok := lookup(name)
	if !ok {
		return nil
	}
	parsed, err := time.ParseDuration(value)
	if err != nil {
		return fmt.Errorf("parse %s: %w", name, err)
	}
	*target = parsed
	return nil
}

func (cfg Config) validate() error {
	if _, err := BuildRedisOptions(cfg); err != nil {
		return err
	}
	if cfg.RateLimitRPS < 0 {
		return errors.New("rate limit RPS cannot be negative")
	}
	if cfg.RateLimitRPS > 0 && cfg.RateLimitBurst < 1 {
		return errors.New("rate limit burst must be at least 1 when rate limiting is enabled")
	}
	if cfg.MaxBodyBytes < 1024 {
		return errors.New("maximum request body must be at least 1024 bytes")
	}
	if cfg.ReadHeaderTimeout <= 0 || cfg.ReadTimeout <= 0 || cfg.WriteTimeout <= 0 || cfg.IdleTimeout <= 0 || cfg.ShutdownTimeout <= 0 {
		return errors.New("HTTP timeouts must be greater than zero")
	}
	return nil
}
