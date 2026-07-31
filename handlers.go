package main

import (
	"time"

	"github.com/gin-gonic/gin"
)

const defaultTTL = time.Hour * 24 * 365 // 默认过期时间，1年
const defaultRenewTime = time.Hour * 48 // 默认续命时间，2天
const defaultShortKeyLength = 7         // 默认短链接长度，7位

// ShortToLongHandler gets the long URL from a short URL
func ShortToLongHandler() gin.HandlerFunc {
	return func(c *gin.Context) {
		resp := Response{}
		shortKey := c.Param("shortKey")
		longURL := ShortToLong(c, shortKey)
		if longURL == "" {
			resp.Code = ResponseCodeServerError
			resp.Msg = "failed to get long URL, please check the short URL if exists or expired"

			c.JSON(404, resp)
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

		// generate short key
		if req.ShortKey == "" {
			req.ShortKey = GenerateRandomString(defaultShortKeyLength)
		}
		// check whether short key exists
		exists, err := CheckRedisKeyIfExist(c, req.ShortKey)
		if err != nil {
			resp.Code = ResponseCodeServerError
			resp.Msg = "failed to check short key"
			logger.Error("failed to check short key: ", err.Error())

			c.JSON(200, resp)
			return
		}
		if exists {
			resp.Code = ResponseCodeParamsCheckError
			resp.Msg = "short key already exists, please use another one or leave it empty to generate automatically"

			logger.Info("short key already exists: ", req.ShortKey)
			c.JSON(200, resp)
			return
		}

		options := &LongToShortOptions{
			ShortKey:   req.ShortKey,
			URL:        req.LongUrl,
			expiration: defaultTTL,
		}
		if err := LongToShort(c, options); err != nil {
			resp.Code = ResponseCodeServerError
			resp.Msg = "failed to create short URL"
			logger.Warn("failed to create short URL: ", err.Error())

			c.JSON(200, resp)
			return
		}

		shortURL := cfg.Proto + "://" + cfg.Domain + "/" + options.ShortKey

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
