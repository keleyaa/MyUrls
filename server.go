package main

import (
	"context"
	"fmt"
	"net/http"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// Dependencies collects runtime dependencies for HTTP handlers.
// Ping is reserved for future health-check injection.
type Dependencies struct {
	Ping func(context.Context) error
}

// NewRouter builds the application's HTTP routes.
func NewRouter(cfg Config, _ Dependencies) *gin.Engine {
	router := gin.Default()
	router.Use(initServiceLogger())

	router.LoadHTMLGlob("public/*.html")
	router.StaticFile("/logo.png", "public/logo.png")
	router.GET("/", func(c *gin.Context) {
		c.HTML(http.StatusOK, "index.html", gin.H{"title": "MyUrls"})
	})

	var limiter *rate.Limiter
	if cfg.RateLimitRPS > 0 && cfg.RateLimitBurst > 0 {
		limiter = rate.NewLimiter(rate.Limit(cfg.RateLimitRPS), cfg.RateLimitBurst)
	}
	router.POST("/short",
		AuthMiddleware(cfg.APIToken),
		RateLimitMiddleware(limiter),
		BodyLimitMiddleware(int64(cfg.MaxBodyBytes)),
		LongToShortHandler(cfg),
	)
	router.GET("/:shortKey", ShortToLongHandler())

	return router
}

func run(cfg Config) {
	gin.SetMode(gin.ReleaseMode)
	router := NewRouter(cfg, Dependencies{})

	logger.Infof("server running on :%s", cfg.Port)
	if err := router.Run(fmt.Sprintf(":%s", cfg.Port)); err != nil {
		logger.Errorw("server stopped", "error", err)
	}
}
