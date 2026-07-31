package main

import (
	"crypto/rand"
	"fmt"
	"io"
)

const letterBytes = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"

// GenerateRandomString returns a cryptographically random alpha-numeric string.
func GenerateRandomString(length int) (string, error) {
	return generateRandomString(rand.Reader, length)
}

func generateRandomString(reader io.Reader, length int) (string, error) {
	if length < 0 {
		return "", fmt.Errorf("random string length must not be negative")
	}

	result := make([]byte, length)
	randomBytes := make([]byte, length)
	limit := byte(256 / len(letterBytes) * len(letterBytes))

	for written := 0; written < length; {
		if _, err := io.ReadFull(reader, randomBytes); err != nil {
			return "", err
		}
		for _, randomByte := range randomBytes {
			if randomByte >= limit {
				continue
			}
			result[written] = letterBytes[randomByte%byte(len(letterBytes))]
			written++
			if written == length {
				break
			}
		}
	}

	return string(result), nil
}
