package main

import (
	"bytes"
	"crypto/sha256"
	"crypto/subtle"
	"io"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// AuthMiddleware optionally protects routes with a bearer token.
func AuthMiddleware(token string) gin.HandlerFunc {
	tokenHash := sha256.Sum256([]byte(token))

	return func(c *gin.Context) {
		if token == "" {
			return
		}

		candidate, ok := strings.CutPrefix(c.GetHeader("Authorization"), "Bearer ")
		if !ok {
			writeUnauthorized(c, false)
			return
		}

		candidateHash := sha256.Sum256([]byte(candidate))
		if subtle.ConstantTimeCompare(tokenHash[:], candidateHash[:]) != 1 {
			writeUnauthorized(c, true)
			return
		}
	}
}

func writeUnauthorized(c *gin.Context, invalidToken bool) {
	challenge := `Bearer realm="MyUrls"`
	if invalidToken {
		challenge += `, error="invalid_token"`
	}
	c.Header("WWW-Authenticate", challenge)
	c.AbortWithStatusJSON(http.StatusUnauthorized, Response{Code: ResponseCodeUnauthorized, Msg: "unauthorized"})
}

// RateLimitMiddleware rejects requests when the supplied limiter has no token.
func RateLimitMiddleware(limiter *rate.Limiter) gin.HandlerFunc {
	return func(c *gin.Context) {
		if limiter == nil || limiter.Allow() {
			return
		}

		c.AbortWithStatusJSON(http.StatusTooManyRequests, Response{Code: ResponseCodeRateLimited, Msg: "rate limit exceeded"})
	}
}

// BodyLimitMiddleware rejects oversized bodies before handlers can process them.
func BodyLimitMiddleware(maxBytes int64) gin.HandlerFunc {
	return func(c *gin.Context) {
		if maxBytes <= 0 || c.Request.Body == nil {
			return
		}
		if c.Request.ContentLength > maxBytes {
			writeBusinessError(c, ResponseCodeParamsCheckError, "request body too large")
			c.Abort()
			return
		}

		body, err := io.ReadAll(io.LimitReader(c.Request.Body, maxBytes+1))
		if err != nil || int64(len(body)) > maxBytes {
			writeBusinessError(c, ResponseCodeParamsCheckError, "request body too large")
			c.Abort()
			return
		}

		c.Request.ContentLength = int64(len(body))
		c.Request.Body = http.MaxBytesReader(c.Writer, io.NopCloser(bytes.NewReader(body)), maxBytes)
	}
}
