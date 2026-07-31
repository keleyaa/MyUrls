package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
	"gopkg.in/natefinch/lumberjack.v2"
)

var logger *zap.SugaredLogger

var (
	requestLoggerMu     sync.Mutex
	requestLogger       *zap.Logger
	requestLoggerWriter *lumberjack.Logger
)

const (
	logFileMaxSize    = 50    // 日志文件最大大小（MB）
	logFileMaxBackups = 10    // 最多保留的备份文件数量
	logFileMaxAge     = 7     // 日志文件最长保留天数
	logFileCompress   = false // 是否压缩备份文件
)

func InitLogger() {
	// 创建 logs 目录
	if err := createLogPath(); err != nil {
		panic("create log path failed: " + err.Error())
	}

	// 初始化 zap logger
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

// createLogPath 创建 logs 目录
func createLogPath() error {
	logFilePath, err := getLogPath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(logFilePath, 0o755); err != nil {
		return fmt.Errorf("create log directory: %w", err)
	}
	return nil
}

// getLogPath 获取 logs 目录
func getLogPath() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", fmt.Errorf("get working directory: %w", err)
	}
	return filepath.Join(dir, "logs"), nil
}

// 定义 zap logger
func initZapLogger() {
	encoder := getEncoder()
	core := zapcore.NewCore(encoder, zapcore.AddSync(os.Stdout), zapcore.DebugLevel)

	logger = zap.New(core).Sugar()
}

// getEncoder 获取 zap encoder
func getEncoder() zapcore.Encoder {
	return zapcore.NewConsoleEncoder(zap.NewDevelopmentEncoderConfig())
}

// initGinLogger 初始化 gin logger
func initGinLogger() *zap.Logger {
	requestLoggerMu.Lock()
	defer requestLoggerMu.Unlock()
	if requestLogger != nil {
		return requestLogger
	}

	logPath, err := getLogPath()
	if err != nil {
		panic("get log path failed: " + err.Error())
	}
	logFileName := "access.log"

	// 日志文件
	logFile := filepath.Join(logPath, logFileName)

	lumberJackLogger := &lumberjack.Logger{
		Filename:   logFile,
		MaxSize:    logFileMaxSize,    // 日志文件最大大小（MB）
		MaxBackups: logFileMaxBackups, // 最多保留的备份文件数量
		MaxAge:     logFileMaxAge,     // 日志文件最长保留天数
		Compress:   logFileCompress,   // 是否压缩备份文件
	}
	writeSyncer := zapcore.AddSync(lumberJackLogger)

	encoderConfig := zap.NewProductionEncoderConfig()
	encoderConfig.EncodeTime = zapcore.ISO8601TimeEncoder
	encoderConfig.EncodeLevel = zapcore.CapitalLevelEncoder
	encoderConfig.EncodeCaller = nil
	encoderConfig.EncodeDuration = zapcore.SecondsDurationEncoder
	encoder := zapcore.NewConsoleEncoder(encoderConfig)

	core := zapcore.NewCore(encoder, writeSyncer, zapcore.InfoLevel)

	requestLoggerWriter = lumberJackLogger
	requestLogger = zap.New(core, zap.AddCaller())
	return requestLogger
}

// CloseRequestLogger releases the shared access-log writer. It is safe to call
// repeatedly and is invoked after the HTTP server has stopped accepting work.
func CloseRequestLogger() error {
	requestLoggerMu.Lock()
	requestLog := requestLogger
	writer := requestLoggerWriter
	requestLogger = nil
	requestLoggerWriter = nil
	requestLoggerMu.Unlock()

	var syncFunc func() error
	if requestLog != nil {
		syncFunc = requestLog.Sync
	}
	var closeFunc func() error
	if writer != nil {
		closeFunc = writer.Close
	}
	return closeRequestLog(syncFunc, closeFunc)
}

func closeRequestLog(syncFunc func() error, closeFunc func() error) error {
	var syncErr error
	if syncFunc != nil {
		syncErr = syncFunc()
	}
	var closeErr error
	if closeFunc != nil {
		closeErr = closeFunc()
	}
	return errors.Join(syncErr, closeErr)
}

// initServiceLogger 初始化服务日志
func initServiceLogger() gin.HandlerFunc {
	_logger := initGinLogger()
	return func(c *gin.Context) {
		start := time.Now()
		path := c.Request.URL.Path

		c.Next()

		_logger.Info(
			"request",
			zap.String("time", start.Format(time.RFC3339)),
			zap.String("method", c.Request.Method),
			zap.String("ip", c.ClientIP()),
			zap.String("user-agent", c.Request.UserAgent()),
			zap.String("path", path),
			zap.Int("status", c.Writer.Status()),
			zap.Duration("latency", time.Since(start)),
		)
	}
}
