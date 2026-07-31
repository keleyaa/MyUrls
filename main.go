package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"syscall"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

func main() {
	cfg, err := LoadConfig(os.Args[1:], os.LookupEnv)
	if errors.Is(err, flag.ErrHelp) {
		return
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(2)
	}
	if cfg.Healthcheck {
		if err := RunHealthcheck(context.Background(), cfg.Port); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		return
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	gin.SetMode(gin.ReleaseMode)
	if RuntimeExitCode(ctx, cfg, productionRuntimeDependencies()) != runtimeSuccessExitCode {
		os.Exit(runtimeFailureExitCode)
	}
}

const (
	runtimeSuccessExitCode = 0
	runtimeFailureExitCode = 1
)

// RuntimeDependencies separates process wiring from the application's runtime
// lifecycle so failures can be tested without calling os.Exit.
type RuntimeDependencies struct {
	InitLogger    func()
	SyncLogger    func() error
	InitRedis     func(Config)
	PingRedis     func(context.Context) error
	CloseRedis    func() error
	NewRouter     func(Config, Dependencies) http.Handler
	NewHTTPServer func(Config, http.Handler) *HTTPServer
}

func productionRuntimeDependencies() RuntimeDependencies {
	return RuntimeDependencies{
		InitLogger: InitLogger,
		SyncLogger: SyncLogger,
		InitRedis: func(cfg Config) {
			initRedisClient(&redis.Options{Addr: cfg.RedisAddr, Password: cfg.RedisPassword, DB: 0})
		},
		PingRedis: func(ctx context.Context) error {
			client := GetRedisClient()
			if client == nil {
				return ErrRedisClientUnavailable
			}
			return client.Ping(ctx).Err()
		},
		CloseRedis:    CloseRedisClient,
		NewRouter:     func(cfg Config, dependencies Dependencies) http.Handler { return NewRouter(cfg, dependencies) },
		NewHTTPServer: NewHTTPServer,
	}
}

// RuntimeExitCode translates a runtime error into the process boundary's
// documented exit status. main is the only caller that invokes os.Exit.
func RuntimeExitCode(ctx context.Context, cfg Config, dependencies RuntimeDependencies) int {
	if err := RunApplication(ctx, cfg, dependencies); err != nil {
		if logger != nil {
			logger.Errorw("application stopped", "error", err)
		}
		return runtimeFailureExitCode
	}
	return runtimeSuccessExitCode
}

// RunApplication owns initialized resources for one application lifetime.
func RunApplication(ctx context.Context, cfg Config, dependencies RuntimeDependencies) (err error) {
	dependencies.InitLogger()
	defer func() {
		if syncErr := dependencies.SyncLogger(); syncErr != nil && err == nil {
			err = fmt.Errorf("sync logger: %w", syncErr)
		}
	}()

	dependencies.InitRedis(cfg)
	defer func() {
		if closeErr := dependencies.CloseRedis(); closeErr != nil && err == nil {
			err = fmt.Errorf("close redis: %w", closeErr)
		}
	}()

	if err = dependencies.PingRedis(ctx); err != nil {
		return fmt.Errorf("redis ping: %w", err)
	}

	router := dependencies.NewRouter(cfg, Dependencies{Ping: dependencies.PingRedis})
	defer func() {
		if closeErr := CloseRequestLogger(); closeErr != nil && err == nil {
			err = fmt.Errorf("close request logger: %w", closeErr)
		}
	}()
	server := dependencies.NewHTTPServer(cfg, router)
	if logger != nil {
		logger.Infof("server running on %s", server.Addr)
	}
	if err = server.Serve(ctx); err != nil {
		return fmt.Errorf("serve HTTP: %w", err)
	}
	return nil
}
