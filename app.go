package main

import (
	"context"
	"errors"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"golang.org/x/time/rate"
)

var (
	ErrShortKeyExists    = errors.New("short key already exists")
	ErrShortKeyExhausted = errors.New("could not generate an available short key")
)

const (
	defaultTTL            = time.Hour * 24 * 365
	defaultShortKeyLength = 7
)

type shortKeyGenerator func(int) (string, error)

type App struct {
	cfg      Config
	store    *Store
	generate shortKeyGenerator
}

func NewApp(cfg Config, store *Store) *App {
	return &App{cfg: cfg, store: store, generate: GenerateRandomString}
}

func (a *App) CreateShortURL(ctx context.Context, requestedKey, longURL string) (string, error) {
	return a.createShortURL(ctx, requestedKey, longURL, a.generate)
}

func (a *App) createShortURL(ctx context.Context, requestedKey, longURL string, generate shortKeyGenerator) (string, error) {
	if requestedKey != "" {
		created, err := a.store.StoreShortURL(ctx, requestedKey, longURL, defaultTTL)
		if err != nil {
			return "", err
		}
		if !created {
			return "", ErrShortKeyExists
		}
		return requestedKey, nil
	}

	for range 5 {
		shortKey, err := generate(defaultShortKeyLength)
		if err != nil {
			return "", err
		}
		created, err := a.store.StoreShortURL(ctx, shortKey, longURL, defaultTTL)
		if err != nil {
			return "", err
		}
		if created {
			return shortKey, nil
		}
	}
	return "", ErrShortKeyExhausted
}

func (a *App) ResolveShortURL(ctx context.Context, shortKey string) (string, error) {
	return a.store.LoadLongURL(ctx, shortKey)
}

type LongToShortParams struct {
	LongUrl  string `form:"longUrl" json:"longUrl" binding:"required"`
	ShortKey string `form:"shortKey" json:"shortKey" binding:"omitempty"`
}

func (a *App) Router() *gin.Engine {
	router := gin.New()
	router.Use(privacySafeRecovery())
	router.Use(initServiceLogger())
	router.LoadHTMLGlob("public/*.html")
	router.StaticFile("/fonts/manrope-latin-wght-normal.woff2", "public/fonts/manrope-latin-wght-normal.woff2")
	router.StaticFile("/app.js", "public/app.js")
	router.StaticFile("/styles.css", "public/styles.css")
	router.GET("/", func(c *gin.Context) {
		c.HTML(http.StatusOK, "index.html", gin.H{"title": "MyUrls"})
	})

	var limiter *rate.Limiter
	if a.cfg.RateLimitRPS > 0 && a.cfg.RateLimitBurst > 0 {
		limiter = rate.NewLimiter(rate.Limit(a.cfg.RateLimitRPS), a.cfg.RateLimitBurst)
	}
	router.POST(
		"/short",
		AuthMiddleware(a.cfg.APIToken),
		RateLimitMiddleware(limiter),
		BodyLimitMiddleware(int64(a.cfg.MaxBodyBytes)),
		a.longToShortHandler(),
	)
	router.GET("/healthz", HealthHandler(a.store.Ping))
	router.GET("/:shortKey", a.shortToLongHandler())
	return router
}

func (a *App) shortToLongHandler() gin.HandlerFunc {
	return func(c *gin.Context) {
		longURL, err := a.ResolveShortURL(c, c.Param("shortKey"))
		if errors.Is(err, redis.Nil) {
			c.JSON(http.StatusNotFound, Response{
				Code: ResponseCodeServerError,
				Msg:  "failed to get long URL, please check the short URL if exists or expired",
			})
			return
		}
		if err != nil {
			if logger != nil {
				logger.Warn("failed to resolve short URL")
			}
			c.JSON(http.StatusInternalServerError, Response{
				Code: ResponseCodeServerError,
				Msg:  "failed to get long URL",
			})
			return
		}
		c.Redirect(http.StatusMovedPermanently, longURL)
	}
}

func (a *App) longToShortHandler() gin.HandlerFunc {
	return func(c *gin.Context) {
		var req LongToShortParams
		if err := c.ShouldBind(&req); err != nil {
			var maxBytesErr *http.MaxBytesError
			if errors.As(err, &maxBytesErr) {
				writeBusinessError(c, ResponseCodeParamsCheckError, "request body too large")
			} else {
				writeBusinessError(c, ResponseCodeParamsCheckError, "invalid parameters")
			}
			return
		}

		normalized, err := NormalizeLongURL(req.LongUrl)
		if err != nil {
			writeBusinessError(c, ResponseCodeParamsCheckError, "invalid long URL")
			return
		}
		if req.ShortKey != "" && ValidateShortKey(req.ShortKey) != nil {
			writeBusinessError(c, ResponseCodeParamsCheckError, "invalid short key")
			return
		}

		shortKey, err := a.CreateShortURL(c, req.ShortKey, normalized)
		if errors.Is(err, ErrShortKeyExists) {
			writeBusinessError(c, ResponseCodeParamsCheckError, "short key already exists, please use another one or leave it empty to generate automatically")
			return
		}
		if err != nil {
			if logger != nil {
				logger.Warn("failed to create short URL")
			}
			c.JSON(http.StatusOK, Response{Code: ResponseCodeServerError, Msg: "failed to create short URL"})
			return
		}

		c.JSON(http.StatusOK, gin.H{
			"Code":     ResponseCodeSuccessLegacy,
			"ShortUrl": a.cfg.ShortURL(shortKey),
		})
	}
}

func writeBusinessError(c *gin.Context, code int, message string) {
	c.JSON(http.StatusOK, Response{Code: code, Msg: message})
}
