package main

import (
	"context"
	"errors"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// Dependencies collects runtime dependencies for HTTP handlers.
type Dependencies struct {
	Ping func(context.Context) error
}

// HTTPServer keeps the HTTP server and its graceful shutdown deadline together.
type HTTPServer struct {
	*http.Server
	ShutdownTimeout time.Duration
	listenAndServe  func() error
	shutdown        func(context.Context) error
}

// NewRouter builds the application's HTTP routes.
func NewRouter(cfg Config, dependencies Dependencies) *gin.Engine {
	router := gin.Default()
	router.Use(initServiceLogger())

	router.LoadHTMLGlob("public/*.html")
	router.StaticFile("/logo.png", "public/logo.png")
	router.StaticFile("/app.js", "public/app.js")
	router.StaticFile("/styles.css", "public/styles.css")
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
	router.GET("/healthz", HealthHandler(dependencies.Ping))
	router.GET("/:shortKey", ShortToLongHandler())

	return router
}

// NewHTTPServer builds an HTTP server with bounded connection lifetimes.
func NewHTTPServer(cfg Config, handler http.Handler) *HTTPServer {
	server := &HTTPServer{
		Server: &http.Server{
			Addr:              ":" + cfg.Port,
			Handler:           handler,
			ReadHeaderTimeout: cfg.ReadHeaderTimeout,
			ReadTimeout:       cfg.ReadTimeout,
			WriteTimeout:      cfg.WriteTimeout,
			IdleTimeout:       cfg.IdleTimeout,
		},
		ShutdownTimeout: cfg.ShutdownTimeout,
	}
	server.listenAndServe = server.Server.ListenAndServe
	server.shutdown = server.Server.Shutdown
	return server
}

// Serve runs the server until it stops or the supplied context requests a
// graceful shutdown. http.ErrServerClosed is the expected shutdown result.
func (server *HTTPServer) Serve(ctx context.Context) error {
	serveErr := make(chan error, 1)
	go func() {
		serveErr <- server.listenAndServe()
	}()

	select {
	case err := <-serveErr:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	case <-ctx.Done():
		shutdownCtx, cancel := context.WithTimeout(context.Background(), server.ShutdownTimeout)
		defer cancel()

		shutdownErr := server.shutdown(shutdownCtx)
		return errors.Join(shutdownErr, normalizeServeError(<-serveErr))
	}
}

func normalizeServeError(err error) error {
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}
