package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"net/http"
	"os"

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

func run(cfg Config) {
	// init and run server
	gin.SetMode(gin.ReleaseMode)
	router := gin.Default()

	// logger
	router.Use(initServiceLogger())

	// static files
	router.LoadHTMLGlob("public/*.html")
	router.StaticFile("/logo.png", "public/logo.png")

	router.GET("/", func(context *gin.Context) {
		context.HTML(http.StatusOK, "index.html", gin.H{
			"title": "MyUrls",
		})
	})

	router.POST("/short", LongToShortHandler(cfg))
	router.GET("/:shortKey", ShortToLongHandler())

	logger.Infof("server running on :%s", cfg.Port)
	router.Run(fmt.Sprintf(":%s", cfg.Port))
}
