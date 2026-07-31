package main

import (
	"crypto/subtle"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
	"golang.org/x/time/rate"
)

// AuthMiddleware optionally protects routes with a bearer token.
func AuthMiddleware(token string) gin.HandlerFunc {
	return func(c *gin.Context) {
		if token == "" {
			return
		}

		authorization := c.GetHeader("Authorization")
		if !strings.HasPrefix(authorization, "Bearer ") ||
			subtle.ConstantTimeCompare([]byte(authorization[len("Bearer "):]), []byte(token)) != 1 {
			c.AbortWithStatusJSON(http.StatusUnauthorized, Response{Code: ResponseCodeUnauthorized, Msg: "unauthorized"})
			return
		}
	}
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

// BodyLimitMiddleware limits the number of bytes read from request bodies.
func BodyLimitMiddleware(maxBytes int64) gin.HandlerFunc {
	return func(c *gin.Context) {
		if maxBytes > 0 && c.Request.Body != nil {
			c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, maxBytes)
		}
	}
}
