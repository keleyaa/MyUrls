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
)

func main() {
	cfg, err := LoadConfig(os.Args[1:], os.LookupEnv)
	if errors.Is(err, flag.ErrHelp) {
		return
	}
	if err != nil {
		fmt.Fprintln(os.Stdout, "invalid configuration")
		os.Exit(2)
	}
	if cfg.Healthcheck {
		if err := RunHealthcheck(context.Background(), cfg.Port); err != nil {
			fmt.Fprintln(os.Stdout, "healthcheck failed")
			os.Exit(1)
		}
		return
	}

	if RuntimeExitCode(context.Background(), cfg, productionRuntimeDependencies()) != runtimeSuccessExitCode {
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
	OpenStore     func(Config) (*Store, error)
	LogError      func(error)
	SignalContext func(context.Context) (context.Context, context.CancelFunc)
	NewApp        func(Config, *Store) *App
	NewHTTPServer func(Config, http.Handler) *HTTPServer
}

func productionRuntimeDependencies() RuntimeDependencies {
	return RuntimeDependencies{
		InitLogger: InitLogger,
		SyncLogger: SyncLogger,
		OpenStore: func(cfg Config) (*Store, error) {
			options, err := BuildRedisOptions(cfg)
			if err != nil {
				return nil, err
			}
			return NewStore(options), nil
		},
		LogError: func(error) {
			if logger != nil {
				logger.Error("application stopped")
			}
		},
		SignalContext: func(parent context.Context) (context.Context, context.CancelFunc) {
			return signal.NotifyContext(parent, os.Interrupt, syscall.SIGTERM)
		},
		NewApp: func(cfg Config, store *Store) *App {
			gin.SetMode(gin.ReleaseMode)
			return NewApp(cfg, store)
		},
		NewHTTPServer: NewHTTPServer,
	}
}

// RuntimeExitCode translates a runtime error into the process boundary's
// documented exit status. main is the only caller that invokes os.Exit.
func RuntimeExitCode(ctx context.Context, cfg Config, dependencies RuntimeDependencies) int {
	if err := RunApplication(ctx, cfg, dependencies); err != nil {
		return runtimeFailureExitCode
	}
	return runtimeSuccessExitCode
}

// RunApplication owns initialized resources for one application lifetime.
func RunApplication(ctx context.Context, cfg Config, dependencies RuntimeDependencies) (err error) {
	dependencies.InitLogger()
	defer func() {
		if err != nil {
			dependencies.LogError(err)
		}
		if syncErr := dependencies.SyncLogger(); syncErr != nil {
			err = errors.Join(err, fmt.Errorf("sync logger: %w", syncErr))
		}
	}()

	store, openErr := dependencies.OpenStore(cfg)
	if store != nil {
		defer func() {
			if closeErr := store.Close(); closeErr != nil {
				err = errors.Join(err, fmt.Errorf("close redis: %w", closeErr))
			}
		}()
	}
	if openErr != nil || store == nil {
		return errors.New("initialize Redis client failed")
	}

	if err = store.Ping(ctx); err != nil {
		return fmt.Errorf("redis ping: %w", err)
	}

	serveCtx, stop := dependencies.SignalContext(ctx)
	defer stop()
	app := dependencies.NewApp(cfg, store)
	if app == nil {
		return errors.New("initialize application failed")
	}
	server := dependencies.NewHTTPServer(cfg, app.Router())
	if logger != nil {
		logger.Infof("server running on %s", server.Addr)
	}
	if err = server.Serve(serveCtx); err != nil {
		return fmt.Errorf("serve HTTP: %w", err)
	}
	return nil
}
