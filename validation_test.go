package main

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNormalizeLongURL(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		want    string
		wantErr bool
	}{
		{"plain https", "https://example.com/a?q=1", "https://example.com/a?q=1", false},
		{"plain http", "http://example.com", "http://example.com", false},
		{"legacy base64", "aHR0cHM6Ly9leGFtcGxlLmNvbQ==", "https://example.com", false},
		{"javascript", "javascript:alert(1)", "", true},
		{"missing host", "https:///path", "", true},
		{"credentials", "https://user:pass@example.com", "", true},
		{"control char", "https://example.com/\nnext", "", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := NormalizeLongURL(tt.input)
			if tt.wantErr {
				require.Error(t, err)
				return
			}
			require.NoError(t, err)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestValidateShortKey(t *testing.T) {
	valid := []string{"a", "A1_-", strings.Repeat("x", 64)}
	for _, value := range valid {
		t.Run("valid "+value[:1], func(t *testing.T) {
			require.NoError(t, ValidateShortKey(value))
		})
	}

	invalid := []string{
		"", strings.Repeat("x", 65), "has/slash", "has space",
		"healthz", "logo.png", "app.js", "styles.css",
	}
	for _, value := range invalid {
		t.Run("invalid "+value, func(t *testing.T) {
			require.Error(t, ValidateShortKey(value))
		})
	}
}
