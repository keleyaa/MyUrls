package main

import (
	"context"
	"errors"
	"io"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

// HTTPServer keeps the HTTP server and its graceful shutdown deadline together.
type HTTPServer struct {
	*http.Server
	ShutdownTimeout time.Duration
	listenAndServe  func() error
	shutdown        func(context.Context) error
}

// privacySafeRecovery prevents Gin's default recovery logger from writing a
// request dump. Request lines, query strings, and headers can contain short
// URLs or credentials, so recovery output is deliberately reduced to a fixed
// application event.
func privacySafeRecovery() gin.HandlerFunc {
	return gin.CustomRecoveryWithWriter(io.Discard, func(c *gin.Context, _ any) {
		if logger != nil {
			logger.Error("request panic recovered")
		}
		c.AbortWithStatus(http.StatusInternalServerError)
	})
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
