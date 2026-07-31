package main

import (
	"encoding/base64"
	"errors"
	"net/url"
	"regexp"
	"strings"
	"unicode"
	"unicode/utf8"
)

var (
	errInvalidURL      = errors.New("invalid URL")
	errInvalidShortKey = errors.New("invalid short key")
	shortKeyPattern    = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)
	reservedShortKeys  = map[string]struct{}{
		"healthz":    {},
		"logo.png":   {},
		"app.js":     {},
		"styles.css": {},
	}
)

func NormalizeLongURL(raw string) (string, error) {
	if err := validateHTTPURL(raw); err == nil {
		return raw, nil
	}

	decoded, err := base64.StdEncoding.DecodeString(raw)
	if err != nil {
		return "", errInvalidURL
	}
	value := string(decoded)
	if err := validateHTTPURL(value); err != nil {
		return "", errInvalidURL
	}
	return value, nil
}

func validateHTTPURL(raw string) error {
	if raw == "" || !utf8.ValidString(raw) {
		return errInvalidURL
	}
	for _, value := range raw {
		if unicode.IsControl(value) {
			return errInvalidURL
		}
	}

	parsed, err := url.Parse(raw)
	if err != nil || !parsed.IsAbs() || parsed.Host == "" || parsed.User != nil {
		return errInvalidURL
	}
	switch strings.ToLower(parsed.Scheme) {
	case "http", "https":
		return nil
	default:
		return errInvalidURL
	}
}

func ValidateShortKey(key string) error {
	if !shortKeyPattern.MatchString(key) {
		return errInvalidShortKey
	}
	if _, reserved := reservedShortKeys[strings.ToLower(key)]; reserved {
		return errInvalidShortKey
	}
	return nil
}
