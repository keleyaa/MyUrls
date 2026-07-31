package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"os"

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

	InitLogger()

	// init and check redis
	initRedisClient(&redis.Options{
		Addr:     cfg.RedisAddr,
		Password: cfg.RedisPassword,
		DB:       0,
	})

	ctx := context.Background()
	rc := GetRedisClient()
	rs := rc.Ping(ctx)
	if rs.Err() != nil {
		logger.Fatalln("redis ping failed: ", rs.Err())
	}
	logger.Info("redis ping success")

	// GC optimize
	ballast := make([]byte, 1<<30) // 预分配 1G 内存，不会实际占用物理内存，不可读写该变量
	defer func() {
		logger.Info("ballast len %v", len(ballast))
	}()

	// start http server
	run(cfg)
}
