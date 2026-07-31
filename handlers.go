package main

import (
	"errors"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

const defaultRenewTime = time.Hour * 48 // 默认续命时间，2天
const defaultShortKeyLength = 7         // 默认短链接长度，7位

// ShortToLongHandler gets the long URL from a short URL
func ShortToLongHandler() gin.HandlerFunc {
	return func(c *gin.Context) {
		resp := Response{}
		shortKey := c.Param("shortKey")
		longURL, err := ResolveShortURL(c, shortKey)
		if errors.Is(err, redis.Nil) {
			resp.Code = ResponseCodeServerError
			resp.Msg = "short URL not found or expired"

			c.JSON(http.StatusNotFound, resp)
			return
		}
		if err != nil {
			resp.Code = ResponseCodeServerError
			resp.Msg = "failed to get long URL"

			c.JSON(http.StatusInternalServerError, resp)
			return
		}

		// todo
		// check whether need renew expiration time
		// only renew once per day
		// if err := Renew(c, shortKey, defaultRenewTime); err != nil {
		// 	logger.Warn("failed to renew short URL: ", err.Error())
		// }

		c.Redirect(301, longURL)
	}
}

type LongToShortParams struct {
	LongUrl  string `form:"longUrl" json:"longUrl" binding:"required"`
	ShortKey string `form:"shortKey" json:"shortKey" binding:"omitempty"`
}

// LongToShortHandler creates a short URL from a long URL
func LongToShortHandler(cfg Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		resp := Response{}

		// check parameters
		req := LongToShortParams{}
		if err := c.ShouldBind(&req); err != nil {
			resp.Code = ResponseCodeParamsCheckError
			resp.Msg = "invalid parameters"
			logger.Warn("invalid parameters: ", err.Error())

			c.JSON(200, resp)
			return
		}

		normalized, err := NormalizeLongURL(req.LongUrl)
		if err != nil {
			writeBusinessError(c, ResponseCodeParamsCheckError, "invalid long URL")
			return
		}
		req.LongUrl = normalized
		if req.ShortKey != "" {
			if err := ValidateShortKey(req.ShortKey); err != nil {
				writeBusinessError(c, ResponseCodeParamsCheckError, "invalid short key")
				return
			}
		}

		shortKey, err := CreateShortURL(c, req.ShortKey, req.LongUrl)
		if errors.Is(err, ErrShortKeyExists) {
			resp.Code = ResponseCodeParamsCheckError
			resp.Msg = "short key already exists, please use another one or leave it empty to generate automatically"

			logger.Info("short key already exists: ", req.ShortKey)
			c.JSON(200, resp)
			return
		}
		if err != nil {
			resp.Code = ResponseCodeServerError
			resp.Msg = "failed to create short URL"
			logger.Warn("failed to create short URL: ", err.Error())

			c.JSON(200, resp)
			return
		}

		shortURL := cfg.Proto + "://" + cfg.Domain + "/" + shortKey

		// 兼容以前的返回结构体
		respDataLegacy := gin.H{
			"Code":     ResponseCodeSuccessLegacy,
			"ShortUrl": shortURL,
		}
		c.JSON(200, respDataLegacy)
	}
}

func writeBusinessError(c *gin.Context, code int, message string) {
	c.JSON(200, Response{Code: code, Msg: message})
}
