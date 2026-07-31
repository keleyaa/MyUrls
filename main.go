package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
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
		if err := RunHealthcheck(cfg.Port); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		return
	}

	InitLogger()
	defer SyncLogger()

	// init and check redis
	initRedisClient(&redis.Options{
		Addr:     cfg.RedisAddr,
		Password: cfg.RedisPassword,
		DB:       0,
	})

	defer func() {
		if err := CloseRedisClient(); err != nil {
			logger.Warnw("redis close failed", "error", err)
		}
	}()

	if err := GetRedisClient().Ping(context.Background()).Err(); err != nil {
		logger.Errorw("redis ping failed", "error", err)
		return
	}
	logger.Info("redis ping success")

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	gin.SetMode(gin.ReleaseMode)
	router := NewRouter(cfg, Dependencies{Ping: func(ctx context.Context) error {
		return GetRedisClient().Ping(ctx).Err()
	}})
	server := NewHTTPServer(cfg, router)

	logger.Infof("server running on %s", server.Addr)
	if err := server.Serve(ctx); err != nil {
		logger.Errorw("server stopped", "error", err)
	}
}
