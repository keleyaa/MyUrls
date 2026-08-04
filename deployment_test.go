package main

import (
	"os"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestContainerDefinesChinaTimezone(t *testing.T) {
	dockerfile, err := os.ReadFile("Dockerfile")
	assert.NoError(t, err)
	content := string(dockerfile)

	assert.Contains(t, content, "ENV TZ=Asia/Shanghai")
	assert.Contains(t, content, "/usr/share/zoneinfo/Asia/Shanghai")
	assert.Contains(t, content, "/etc/localtime")
}

func TestComposeLimitsContainerLogs(t *testing.T) {
	compose, err := os.ReadFile("docker-compose.yaml")
	assert.NoError(t, err)
	content := string(compose)

	assert.Contains(t, content, "x-logging:")
	assert.Contains(t, content, "driver: json-file")
	assert.Contains(t, content, `max-size: "10m"`)
	assert.Contains(t, content, `max-file: "3"`)
	assert.Equal(t, 2, strings.Count(content, "logging: *default-logging"))
}

func TestComposeDefinesChinaTimezoneForEveryService(t *testing.T) {
	compose, err := os.ReadFile("docker-compose.yaml")
	assert.NoError(t, err)

	assert.Equal(t, 2, strings.Count(string(compose), "TZ: Asia/Shanghai"))
}

func TestDocumentationExplainsPrivacySafeLogging(t *testing.T) {
	for _, path := range []string{"README.md", "docs/operations.md"} {
		contentBytes, err := os.ReadFile(path)
		assert.NoError(t, err)
		content := string(contentBytes)

		assert.Contains(t, content, "Asia/Shanghai", path)
		assert.Contains(t, content, "路由模板", path)
		assert.Contains(t, content, "成功的 `/healthz`", path)
		assert.Contains(t, content, "固定事件", path)
		assert.Contains(t, content, "10 MB", path)
		assert.Contains(t, content, "3 个", path)
	}
}
