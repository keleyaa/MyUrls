package main

import (
	"errors"
	"net/http"
	"os"
	"sync"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

var logger *zap.SugaredLogger

var (
	requestLoggerMu sync.Mutex
	requestLogger   *zap.Logger
)

var chinaStandardTime = time.FixedZone("CST", 8*60*60)

func InitLogger() {
	initZapLogger()
}

// SyncLogger flushes buffered log entries when the process exits. Syncing a
// terminal stream is unsupported on some platforms, but other failures are
// returned to the caller that owns process shutdown.
func SyncLogger() error {
	if logger == nil {
		return nil
	}
	err := logger.Sync()
	if errors.Is(err, syscall.EINVAL) {
		return nil
	}
	return err
}

// 定义 zap logger
func initZapLogger() {
	encoder := getEncoder()
	core := zapcore.NewCore(encoder, zapcore.AddSync(os.Stdout), zapcore.DebugLevel)

	logger = zap.New(core).Sugar()
}

// getEncoder 获取 zap encoder
func getEncoder() zapcore.Encoder {
	config := zap.NewDevelopmentEncoderConfig()
	config.EncodeTime = encodeChinaTime
	return zapcore.NewConsoleEncoder(config)
}

// initGinLogger 初始化 gin logger
func initGinLogger() *zap.Logger {
	requestLoggerMu.Lock()
	defer requestLoggerMu.Unlock()
	if requestLogger != nil {
		return requestLogger
	}

	writeSyncer := zapcore.AddSync(os.Stdout)

	encoderConfig := zap.NewProductionEncoderConfig()
	encoderConfig.EncodeTime = encodeChinaTime
	encoderConfig.EncodeLevel = zapcore.CapitalLevelEncoder
	encoderConfig.EncodeCaller = nil
	encoderConfig.EncodeDuration = zapcore.SecondsDurationEncoder
	encoder := zapcore.NewConsoleEncoder(encoderConfig)

	core := zapcore.NewCore(encoder, writeSyncer, zapcore.InfoLevel)

	requestLogger = zap.New(core, zap.AddCaller())
	return requestLogger
}

// initServiceLogger 初始化服务日志
func initServiceLogger() gin.HandlerFunc {
	return serviceLoggerMiddleware(initGinLogger())
}

func formatChinaTime(value time.Time) string {
	return value.In(chinaStandardTime).Format(time.RFC3339Nano)
}

func encodeChinaTime(value time.Time, encoder zapcore.PrimitiveArrayEncoder) {
	encoder.AppendString(formatChinaTime(value))
}

func privacySafeRoute(fullPath string) string {
	if fullPath == "" {
		return "unmatched"
	}
	return fullPath
}

func shouldLogRequest(route string, status int) bool {
	return route != "/healthz" || status >= http.StatusBadRequest
}

func serviceLoggerMiddleware(requestLog *zap.Logger) gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()

		c.Next()

		route := privacySafeRoute(c.FullPath())
		status := c.Writer.Status()
		if !shouldLogRequest(route, status) {
			return
		}

		requestLog.Info(
			"request",
			zap.String("time", formatChinaTime(start)),
			zap.String("method", c.Request.Method),
			zap.String("route", route),
			zap.Int("status", status),
			zap.Duration("latency", time.Since(start)),
		)
	}
}
