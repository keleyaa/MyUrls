# MyUrls 兼容性优先维护实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用
> superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）
> 语法跟踪进度。

**目标：** 在保持现有 CLI、API、重定向和页面工作流兼容的前提下，完成
MyUrls 的依赖升级、安全加固、原子存储、无框架页面、容器和 CI 现代化。

**架构：** 将配置、输入校验、HTTP 传输、短链领域逻辑、Redis 存储和服务
生命周期拆成边界清晰的小文件。创建路径使用后端校验、`crypto/rand` 和 Redis
`SET NX EX`；运行路径继续暴露旧 API，同时增加可选鉴权、限流和健康检查。

**技术栈：** Go 1.25 模块基线、Go 1.26.5 工具链、Gin 1.12、go-redis
9.21、Redis 8.10、Zap 1.28、原生 HTML/CSS/JavaScript、Docker Compose、
GitHub Actions、Playwright 1.62.1。

**设计规格：**
[`docs/superpowers/specs/2026-07-31-myurls-maintenance-design.md`](../specs/2026-07-31-myurls-maintenance-design.md)

---

## 执行约束

- 工作目录固定为 `/Users/li/Desktop/GitHub/MyUrls`。
- 分支必须为 `codex/myurls-maintenance`，远端必须为
  `https://github.com/keleyaa/MyUrls.git`。
- 本机没有 Go；所有本地 Go 命令使用本计划给出的 Go 1.26.5 Docker 命令。
- 不修改 Subweb、SubConverter-Extended 或其他仓库。
- 每个任务先看到预期红灯，再写最少实现，绿灯后才允许重构和提交。
- 每次提交前运行 `git diff --check` 并确认暂存文件只属于当前任务。
- 不推送分支、不创建 PR，除非用户另行授权。

每个新 shell 会话先定义以下只读辅助函数；后续 `go_docker` 命令均调用它：

```sh
go_docker() {
  docker run --rm \
    -v /Users/li/Desktop/GitHub/MyUrls:/app \
    -v myurls-go-mod:/go/pkg/mod \
    -v myurls-go-build:/root/.cache/go-build \
    -w /app \
    golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
    "$@"
}
```

## 文件结构

### 新建生产文件

- `config.go`：配置结构、flag、环境变量、默认值和校验。
- `server.go`：Router、`http.Server`、服务启动与优雅关闭。
- `validation.go`：普通/Base64 URL 与短码校验。
- `middleware.go`：Bearer Token、请求大小和全局创建限流。
- `health.go`：HTTP 健康端点和二进制健康检查客户端。
- `public/app.js`：无依赖表单、请求和剪贴板行为。
- `public/styles.css`：固定、响应式、无外部资源的页面样式。
- `.dockerignore`：Docker 构建上下文排除规则。
- `.github/dependabot.yml`：Go、npm、Docker、Actions 周期更新。
- `package.json`、`package-lock.json`：仅用于 Playwright E2E，不进入运行镜像。
- `playwright.config.js`：桌面和移动端浏览器项目及本地服务地址。
- `tests/e2e/app.spec.js`：真实浏览器兼容工作流。
- `docs/operations.md`：部署、健康、升级、备份与回滚。

### 新建测试文件

- `config_test.go`、`validation_test.go`、`handlers_test.go`。
- `random_test.go`、`middleware_test.go`、`server_test.go`、`health_test.go`。
- `tests/integration/redis_test.go`：真实 Redis 7/8 协议与并发验证。

### 修改文件

- `main.go`：只保留进程编排。
- `handlers.go`：兼容 transport 与错误映射。
- `logic.go`：创建、重试、查询和领域错误。
- `redis.go`：Redis 生命周期和原子存储。
- `random.go`：`crypto/rand` 实现。
- `logger.go`：同步日志、非 root 目录和关闭行为。
- `const.go`：新增可选安全错误码，不修改既有值。
- `go.mod`、`go.sum`：工具链与全部可达模块升级。
- `public/index.html`：无框架语义 HTML。
- `Dockerfile`、`docker-compose.yaml`、`.env.example`、`Makefile`。
- `.github/workflows/go.yml`、`.github/workflows/docker_build_push.yml`。
- `README.md`：安装、运行、配置、镜像和文档入口。

---

### 任务 1：提交已完成的测试隔离修复

**文件：**
- 修改：`logic_test.go`
- 修改：`redis_test.go`
- 修改：`go.mod`
- 修改：`go.sum`

- [ ] **步骤 1：审查现有未提交补丁**

运行：

```sh
git diff -- logic_test.go redis_test.go go.mod go.sum
```

预期：只包含 `miniredis v2.38.0`、`newTestRedisOptions`、
`resetRedisClient` 以及测试调用点，不包含生产代码。

- [ ] **步骤 2：确认原始红灯证据对应当前修复**

原始失败必须与审核记录一致：完整测试先运行逻辑测试后，
`redis_test.go:19` 的 `assert.Nil` 收到非 nil 全局客户端；无 Redis 服务时
`TestLongToShortAndShortToLong` 报 `connect: connection refused`。

- [ ] **步骤 3：在没有外部 Redis 的环境验证绿灯**

运行：

```sh
docker run --rm \
  -v /Users/li/Desktop/GitHub/MyUrls:/app \
  -v myurls-go-mod:/go/pkg/mod \
  -v myurls-go-build:/root/.cache/go-build \
  -w /app \
  golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
  go test -shuffle=on -count=20 ./...
```

预期：`ok github.com/CareyWang/MyUrls`，20 次无失败。

- [ ] **步骤 4：提交测试隔离**

```sh
git add go.mod go.sum logic_test.go redis_test.go
git diff --cached --check
git commit -m "test(redis): 隔离测试客户端与外部服务"
```

---

### 任务 2：升级 Go 工具链和全部模块

**文件：**
- 修改：`go.mod`
- 修改：`go.sum`
- 修改：`Makefile`

- [ ] **步骤 1：记录升级前模块红灯**

运行：

```sh
docker run --rm \
  -v /Users/li/Desktop/GitHub/MyUrls:/app \
  -v myurls-go-mod:/go/pkg/mod \
  -v myurls-go-build:/root/.cache/go-build \
  -w /app \
  golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
  go list -m -u all
```

预期：至少报告 Gin `v1.12.0`、go-redis `v9.21.0`、Testify
`v1.11.1` 和 Zap `v1.28.0` 可升级。

- [ ] **步骤 2：更新 Go 指令和直接依赖**

将 `go.mod` 头部改为：

```go
module github.com/CareyWang/MyUrls

go 1.25.0

toolchain go1.26.5
```

运行：

```sh
docker run --rm \
  -v /Users/li/Desktop/GitHub/MyUrls:/app \
  -v myurls-go-mod:/go/pkg/mod \
  -v myurls-go-build:/root/.cache/go-build \
  -w /app \
  golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
  go get github.com/alicebob/miniredis/v2@v2.38.0 github.com/gin-gonic/gin@v1.12.0 github.com/redis/go-redis/v9@v9.21.0 github.com/stretchr/testify@v1.11.1 go.uber.org/zap@v1.28.0 gopkg.in/natefinch/lumberjack.v2@v2.2.1 golang.org/x/time@v0.15.0
```

- [ ] **步骤 3：解析升级后的全部可达模块**

运行：

```sh
docker run --rm \
  -v /Users/li/Desktop/GitHub/MyUrls:/app \
  -v myurls-go-mod:/go/pkg/mod \
  -v myurls-go-build:/root/.cache/go-build \
  -w /app \
  golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
  sh -c 'go mod tidy && go get -u all && go mod tidy'
```

预期：命令退出 0；`gopher-lua` 解析为 `v1.1.2`，旧的
`go-rendezvous`、`kr/pretty`、`check.v1` 从最终图移除。

- [ ] **步骤 4：增加一致的 Make 质量命令**

在 `Makefile` 增加：

```make
.PHONY: test vet race verify

test:
	@go test -count=1 ./...

vet:
	@go vet ./...

race:
	@go test -race -count=1 ./...

verify: fmt vet test
```

- [ ] **步骤 5：验证升级绿灯**

依次运行容器内命令：

```sh
go_docker go test -count=1 ./...
go_docker go vet ./...
go_docker go build ./...
go_docker go list -m -u all
```

预期：测试、vet、构建退出 0；`go list -m -u all` 不包含 `Update` 字段。

- [ ] **步骤 6：提交依赖升级**

```sh
git add go.mod go.sum Makefile
git diff --cached --check
git commit -m "chore(依赖): 升级 Go 工具链和全部模块"
```

---

### 任务 3：集中配置解析与兼容默认值

**文件：**
- 创建：`config.go`
- 创建：`config_test.go`
- 修改：`main.go`

- [ ] **步骤 1：编写配置红灯测试**

在 `config_test.go` 定义表格测试，覆盖：默认值、原有 flag、原有环境变量覆盖、
新增持续时间、负数限流、burst 小于 1、请求上限小于 1024、非法 duration。
测试使用以下公开边界：

```go
func TestLoadConfigDefaults(t *testing.T) {
	cfg, err := LoadConfig(nil, func(string) (string, bool) { return "", false })
	require.NoError(t, err)
	assert.Equal(t, "8080", cfg.Port)
	assert.Equal(t, "localhost:8080", cfg.Domain)
	assert.Equal(t, "https", cfg.Proto)
	assert.Equal(t, 16_384, cfg.MaxBodyBytes)
	assert.Zero(t, cfg.RateLimitRPS)
}

func TestLoadConfigEnvironmentOverridesFlags(t *testing.T) {
	env := map[string]string{"MYURLS_PORT": "9090"}
	cfg, err := LoadConfig([]string{"-port", "8081"}, mapLookup(env))
	require.NoError(t, err)
	assert.Equal(t, "9090", cfg.Port)
}
```

运行：`go_docker go test -run '^TestLoadConfig' -count=1 ./...`

预期：FAIL，`LoadConfig` 和 `Config` 未定义。

- [ ] **步骤 2：实现配置结构和解析器**

在 `config.go` 定义：

```go
type Config struct {
	Port, Domain, Proto, RedisAddr, RedisPassword string
	APIToken                                     string
	RateLimitRPS                                 float64
	RateLimitBurst, MaxBodyBytes                 int
	ReadHeaderTimeout, ReadTimeout               time.Duration
	WriteTimeout, IdleTimeout, ShutdownTimeout   time.Duration
	Healthcheck                                  bool
}

type LookupEnv func(string) (string, bool)

func LoadConfig(args []string, lookup LookupEnv) (Config, error)
```

`LoadConfig` 使用独立 `flag.FlagSet`，先载入设计规格第 7 节默认值，再解析原有
flag，最后用环境变量覆盖。数字使用 `strconv`，duration 使用
`time.ParseDuration`。校验规则固定为：RPS 不得为负；启用限流时 burst 至少为
1；body 至少 1024；所有 timeout 必须大于 0。

- [ ] **步骤 3：让 main 使用 Config**

`main.go` 改为：

```go
cfg, err := LoadConfig(os.Args[1:], os.LookupEnv)
if errors.Is(err, flag.ErrHelp) {
	return
}
if err != nil {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(2)
}
```

删除包级配置变量和 `parseEnvirons`，但保持原 flag 名称和帮助文本。

- [ ] **步骤 4：验证配置绿灯与兼容帮助**

运行：

```sh
go_docker go test -run '^TestLoadConfig' -count=1 ./...
go_docker go run . -h
```

预期：测试通过；帮助仍列出六个原 flag，并新增 `-healthcheck`。

- [ ] **步骤 5：提交配置边界**

```sh
git add config.go config_test.go main.go
git diff --cached --check
git commit -m "refactor(配置): 集中参数和环境变量解析"
```

---

### 任务 4：后端 URL 与短码校验

**文件：**
- 创建：`validation.go`
- 创建：`validation_test.go`
- 创建：`handlers_test.go`
- 修改：`handlers.go`
- 修改：`const.go`

- [ ] **步骤 1：编写 URL 和短码红灯测试**

在 `validation_test.go` 使用表格测试固定以下输入：

```go
var urlCases = []struct {
	name, input, want string
	wantErr           bool
}{
	{"plain https", "https://example.com/a?q=1", "https://example.com/a?q=1", false},
	{"legacy base64", "aHR0cHM6Ly9leGFtcGxlLmNvbQ==", "https://example.com", false},
	{"javascript", "javascript:alert(1)", "", true},
	{"missing host", "https:///path", "", true},
	{"credentials", "https://user:pass@example.com", "", true},
	{"control char", "https://example.com/\nnext", "", true},
}
```

短码接受 `a`、`A1_-` 和 64 字符边界；拒绝空值（只在自定义校验时）、65 字符、
斜线、空格、`healthz`、`logo.png`、`app.js`、`styles.css`。

运行：`go_docker go test -run '^(TestNormalizeLongURL|TestValidateShortKey)$' -count=1 ./...`

预期：FAIL，校验函数未定义。

- [ ] **步骤 2：实现明确的校验函数**

在 `validation.go` 定义：

```go
var (
	errInvalidURL      = errors.New("invalid URL")
	errInvalidShortKey = errors.New("invalid short key")
	shortKeyPattern    = regexp.MustCompile(`^[A-Za-z0-9_-]{1,64}$`)
)

func NormalizeLongURL(raw string) (string, error)
func ValidateShortKey(key string) error
```

`NormalizeLongURL` 先调用内部 `validateHTTPURL(raw)`；失败后才尝试
`base64.StdEncoding.DecodeString` 并再次调用同一校验。URL 必须绝对、scheme 为
HTTP(S)、Host 非空、User 为 nil，原始字符串不得包含 Unicode 控制字符。

- [ ] **步骤 3：编写 Handler 兼容契约红灯**

`handlers_test.go` 使用 `httptest` 和 miniredis 覆盖表单、JSON、旧 Base64、
`javascript:` 与非法短码。成功断言 HTTP 200、`Code == 1`、`ShortUrl` 字段；
非法输入断言 HTTP 200、`Code == 1001`。

运行：`go_docker go test -run '^TestLongToShortHandler' -count=1 ./...`

预期：危险协议测试失败，因为当前 Handler 会写入 Redis。

- [ ] **步骤 4：在 Handler 接入校验**

绑定成功后按顺序调用：

```go
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
```

删除 Handler 中无条件 Base64 解码块。

- [ ] **步骤 5：验证校验绿灯并提交**

运行：`go_docker go test -run '^(TestNormalizeLongURL|TestValidateShortKey|TestLongToShortHandler)' -count=1 ./...`

预期：全部通过。

```sh
git add validation.go validation_test.go handlers.go handlers_test.go const.go
git diff --cached --check
git commit -m "fix(安全): 在后端校验长链接和短码"
```

---

### 任务 5：安全随机和 Redis 原子创建

**文件：**
- 修改：`random.go`
- 创建：`random_test.go`
- 修改：`redis.go`
- 修改：`logic.go`
- 修改：`logic_test.go`
- 修改：`handlers.go`
- 修改：`handlers_test.go`

- [ ] **步骤 1：编写随机源和原子写入红灯测试**

`random_test.go` 固定 API：

```go
func TestGenerateRandomString(t *testing.T) {
	value, err := GenerateRandomString(7)
	require.NoError(t, err)
	assert.Regexp(t, `^[0-9A-Za-z]{7}$`, value)
}

func TestGenerateRandomStringPropagatesReaderError(t *testing.T) {
	_, err := generateRandomString(errReader{}, 7)
	require.Error(t, err)
}
```

`logic_test.go` 增加 100 goroutine 同时创建 `same-key`，统计恰好一个成功、
99 个 `ErrShortKeyExists`，最后值等于成功请求的 URL。

运行：`go_docker go test -run '^(TestGenerateRandomString|TestCreateShortURLAtomic)$' -count=1 ./...`

预期：随机函数签名不匹配；并发测试发现旧 `EXISTS` + `SETEX` 非原子路径。

- [ ] **步骤 2：实现 crypto/rand**

`random.go` 改为：

```go
func GenerateRandomString(length int) (string, error) {
	return generateRandomString(rand.Reader, length)
}

func generateRandomString(reader io.Reader, length int) (string, error) {
	result := make([]byte, length)
	buffer := make([]byte, length)
	if _, err := io.ReadFull(reader, buffer); err != nil {
		return "", err
	}
	for i, value := range buffer {
		result[i] = letterBytes[int(value)%len(letterBytes)]
	}
	return string(result), nil
}
```

- [ ] **步骤 3：定义存储和领域 API**

在 `logic.go` 定义：

```go
var (
	ErrShortKeyExists    = errors.New("short key already exists")
	ErrShortKeyExhausted = errors.New("failed to allocate short key")
)

func CreateShortURL(ctx context.Context, requestedKey, longURL string) (string, error)
func ResolveShortURL(ctx context.Context, shortKey string) (string, error)
```

在 `redis.go` 定义：

```go
func StoreShortURL(ctx context.Context, key, value string, ttl time.Duration) (bool, error) {
	return GetRedisClient().SetNX(ctx, key, value, ttl).Result()
}

func LoadLongURL(ctx context.Context, key string) (string, error) {
	return GetRedisClient().Get(ctx, key).Result()
}
```

自定义短码只尝试一次；自动短码最多 5 次。`SETNX` 返回 false 时映射
`ErrShortKeyExists`，5 次碰撞映射 `ErrShortKeyExhausted`。

- [ ] **步骤 4：更新 Handler 错误映射**

`LongToShortHandler` 调用 `CreateShortURL`；`ErrShortKeyExists` 保持当前冲突
业务码和消息，其他错误返回 `ResponseCodeServerError`。`ShortToLongHandler` 调用
`ResolveShortURL`：`redis.Nil` 返回 404，其他错误返回 500。

- [ ] **步骤 5：验证原子语义和全量测试**

运行：

```sh
go_docker go test -run '^(TestGenerateRandomString|TestCreateShortURLAtomic|TestLongToShortHandler|TestShortToLongHandler)' -count=1 ./...
go_docker go test -shuffle=on -count=20 ./...
```

预期：全部通过，100 并发只有一个写入。

- [ ] **步骤 6：提交原子创建**

```sh
git add random.go random_test.go redis.go logic.go logic_test.go handlers.go handlers_test.go
git diff --cached --check
git commit -m "fix(redis): 原子创建短码并使用安全随机源"
```

---

### 任务 6：鉴权、请求上限和限流

**文件：**
- 创建：`middleware.go`
- 创建：`middleware_test.go`
- 修改：`handlers.go`
- 修改：`const.go`
- 创建：`server.go`（本任务只创建 Router 工厂，任务 7 再扩展生命周期）

- [ ] **步骤 1：编写中间件红灯测试**

固定以下行为：Token 为空放行；Token 非空时缺失/错误 Bearer 返回 401；正确
Bearer 放行；RPS 0 放行；`rate.NewLimiter(1, 1)` 的第二次立即请求返回 429；
超过 `MaxBodyBytes` 的 `/short` 返回 HTTP 200 和业务码 1001。

公开构造边界：

```go
func AuthMiddleware(token string) gin.HandlerFunc
func RateLimitMiddleware(limiter *rate.Limiter) gin.HandlerFunc
func BodyLimitMiddleware(maxBytes int64) gin.HandlerFunc
```

运行：`go_docker go test -run '^(TestAuthMiddleware|TestRateLimitMiddleware|TestBodyLimitMiddleware)$' -count=1 ./...`

预期：FAIL，三个构造函数未定义。

- [ ] **步骤 2：实现最小中间件**

鉴权解析 `Authorization`，只接受精确 `Bearer ` 前缀，并用
`subtle.ConstantTimeCompare` 比较 Token。限流器为 nil 时放行；拒绝时返回
`ResponseCodeRateLimited`。Body limit 使用：

```go
c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, maxBytes)
```

仅把三个中间件挂在 `POST /short`，不影响首页、跳转和健康检查。
同时创建 `server.go`，定义 `NewRouter(cfg Config, deps Dependencies) *gin.Engine`
和最小 `Dependencies`，集中注册现有路由；本任务不创建 `http.Server`。

- [ ] **步骤 3：增加可选错误码**

保持 0、1、1001、1002 不变，新增：

```go
const (
	ResponseCodeUnauthorized = 1003
	ResponseCodeRateLimited  = 1004
)
```

- [ ] **步骤 4：验证绿灯和兼容默认值**

运行：

```sh
go_docker go test -run '^(TestAuthMiddleware|TestRateLimitMiddleware|TestBodyLimitMiddleware|TestLongToShortHandler)$' -count=1 ./...
go_docker go test -count=1 ./...
```

预期：全部通过；默认配置下旧 `/short` 请求无需 Token。

- [ ] **步骤 5：提交保护中间件**

```sh
git add middleware.go middleware_test.go handlers.go const.go server.go
git diff --cached --check
git commit -m "feat(安全): 增加可选鉴权和创建限流"
```

---

### 任务 7：HTTP 生命周期和健康检查

**文件：**
- 修改：`server.go`
- 创建：`server_test.go`
- 创建：`health.go`
- 创建：`health_test.go`
- 修改：`main.go`
- 修改：`redis.go`
- 修改：`logger.go`

- [ ] **步骤 1：编写 Server 和健康红灯测试**

`server_test.go` 断言 `NewHTTPServer` 的 Addr 和五个 timeout 等于 Config。
`health_test.go` 注入成功与失败 ping：

```go
func TestHealthHandler(t *testing.T) {
	tests := []struct {
		name string
		ping func(context.Context) error
		status int
	}{
		{"ready", func(context.Context) error { return nil }, 200},
		{"redis unavailable", func(context.Context) error { return errors.New("down") }, 503},
	}
	// 对每例调用 HealthHandler(tt.ping)，断言状态和不泄露 "down"。
}
```

运行：`go_docker go test -run '^(TestNewHTTPServer|TestHealthHandler|TestHealthcheckCommand)$' -count=1 ./...`

预期：FAIL，Server 与 Health API 未定义。

- [ ] **步骤 2：实现 Router 和 HTTP Server**

定义：

```go
type Dependencies struct {
	Ping func(context.Context) error
}

func NewRouter(cfg Config, deps Dependencies) *gin.Engine
func NewHTTPServer(cfg Config, handler http.Handler) *http.Server
func Serve(ctx context.Context, server *http.Server, shutdownTimeout time.Duration) error
```

`NewRouter` 注册 `/`、静态资源、`POST /short`、`GET /healthz`、最后注册
`GET /:shortKey`。`Serve` 在 goroutine 运行 `ListenAndServe`，收到 ctx 取消后
调用带 timeout 的 `Shutdown`；只忽略 `http.ErrServerClosed`。

- [ ] **步骤 3：实现健康处理与二进制检查**

定义：

```go
type PingFunc func(context.Context) error

func HealthHandler(ping PingFunc) gin.HandlerFunc
func RunHealthcheck(ctx context.Context, port string) error
```

`RunHealthcheck` 使用 3 秒 `http.Client` 请求
`fmt.Sprintf("http://127.0.0.1:%s/healthz", port)`，只接受 200。

- [ ] **步骤 4：重写 main 编排**

main 按固定顺序执行：LoadConfig；healthcheck 早退；InitLogger；初始化并 Ping
Redis；创建 signal context；创建 Router/Server；Serve；关闭 Redis；`logger.Sync`。
删除 GC ballast 和旧 `run()`。`redis.go` 增加 `CloseRedisClient() error`。

- [ ] **步骤 5：验证服务绿灯**

运行：

```sh
go_docker go test -run '^(TestNewHTTPServer|TestHealthHandler|TestHealthcheckCommand)' -count=1 ./...
go_docker go test -count=1 ./...
go_docker go vet ./...
```

预期：全部退出 0。

- [ ] **步骤 6：提交服务生命周期**

```sh
git add server.go server_test.go health.go health_test.go main.go redis.go logger.go
git diff --cached --check
git commit -m "feat(服务): 增加健康检查和优雅退出"
```

---

### 任务 8：用原生页面移除全部运行时前端依赖

**文件：**
- 修改：`public/index.html`
- 创建：`public/app.js`
- 创建：`public/styles.css`
- 修改：`server.go`
- 修改：`server_test.go`

- [ ] **步骤 1：编写静态资产红灯测试**

在 `server_test.go` 增加：访问 `/`、`/app.js`、`/styles.css` 均返回 200；
读取首页后断言不包含 `unpkg.com`、`jsdelivr.net`、`Vue`、`axios`、
`element-ui`，且包含 `id="long-url"`、`id="short-url"`、`id="status"`。

运行：`go_docker go test -run '^TestStaticAssetsHaveNoRuntimeDependencies$' -count=1 ./...`

预期：FAIL，现有首页包含四个 CDN 依赖且 app.js/styles.css 不存在。

- [ ] **步骤 2：创建语义 HTML**

`public/index.html` 只包含本地 `/styles.css`、`/app.js`、logo、带 label 的
`#long-url`、可选 `#short-key`、只读 `#short-url`、`#shorten-button`、
`#copy-button` 和 `role="status"` 的 `#status`。页面语言设为 `zh-CN`。

- [ ] **步骤 3：实现无依赖交互**

`public/app.js` 定义并使用：

```js
async function createShortURL(longUrl, shortKey) {
  const data = new FormData()
  data.append('longUrl', longUrl)
  data.append('shortKey', shortKey)
  const response = await fetch('/short', { method: 'POST', body: data })
  if (!response.ok) throw new Error('request failed')
  return response.json()
}

async function copyText(value) {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(value)
  const input = document.querySelector('#short-url')
  input.select()
  if (!document.execCommand('copy')) throw new Error('copy failed')
}
```

提交时注册表单 submit、复制按钮和 repo logo 点击事件；请求期间禁用提交按钮；
成功显示短链并尝试自动复制；任何错误只显示稳定用户消息。

- [ ] **步骤 4：实现稳定响应式样式**

`public/styles.css` 使用单列工具布局，容器 `width: min(42rem, calc(100% - 2rem))`，
按钮和输入固定高度，卡片圆角不超过 8px；定义可见 focus ring、disabled、
成功和错误状态。不得使用外部字体、渐变、装饰圆球或 viewport 字体缩放。

- [ ] **步骤 5：验证静态绿灯**

运行：

```sh
go_docker go test -run '^TestStaticAssetsHaveNoRuntimeDependencies$' -count=1 ./...
go_docker go test -count=1 ./...
```

预期：全部通过，首页文本中无外部 URL。

- [ ] **步骤 6：提交无框架页面**

```sh
git add public/index.html public/app.js public/styles.css server.go server_test.go
git diff --cached --check
git commit -m "refactor(页面): 移除运行时前端依赖"
```

---

### 任务 9：加固 Docker、Compose 和 Redis 8 升级路径

**文件：**
- 创建：`.dockerignore`
- 修改：`Dockerfile`
- 修改：`docker-compose.yaml`
- 修改：`.env.example`
- 修改：`logger.go`

- [ ] **步骤 1：记录容器红灯**

运行当前镜像并检查：

```sh
docker build -t myurls:pre-hardening .
docker image inspect myurls:pre-hardening --format '{{.Config.User}} {{json .Config.Healthcheck}}'
```

预期：User 为空，Healthcheck 为 null；`docker compose config` 仍警告顶层
`version`，Redis 为浮动 `redis:7`。

- [ ] **步骤 2：写入确定性 Dockerfile**

使用：

```dockerfile
FROM golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /out/myurls . && \
    mkdir -p /out/logs && chown 65532:65532 /out/logs

FROM scratch
WORKDIR /app
COPY --from=build --chown=65532:65532 /out/myurls /app/myurls
COPY --chown=65532:65532 public /app/public
COPY --from=build --chown=65532:65532 /out/logs /app/logs
USER 65532:65532
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/app/myurls", "-healthcheck"]
ENTRYPOINT ["/app/myurls"]
```

- [ ] **步骤 3：创建 .dockerignore**

内容固定包含：

```text
.git
.github
.env
build
data/redis
logs
*.log
docs
tests
node_modules
```

- [ ] **步骤 4：重写 Compose**

删除 `version`，Redis 使用：

```yaml
image: redis:8.10.0@sha256:c29e49ab2f85760a3827b53882e6dd9f5c6c3f0bb7d724e07bb31cbf275a5236
healthcheck:
  test: ["CMD", "redis-cli", "ping"]
  interval: 5s
  timeout: 3s
  retries: 10
```

MyUrls 使用 `depends_on.myurls-redis.condition: service_healthy`、
`read_only: true`、`security_opt: [no-new-privileges:true]`、
`cap_drop: [ALL]` 和命名 logs volume；Redis 不映射宿主端口。

- [ ] **步骤 5：补齐 .env.example**

写入所有设计配置；`MYURLS_RATE_LIMIT_RPS=5`、
`MYURLS_RATE_LIMIT_BURST=10`、Token 留空、Redis 连接为
`myurls-redis:6379`。

- [ ] **步骤 6：验证容器绿灯**

运行：

```sh
docker compose config
docker compose up -d --build
docker compose ps
docker inspect myurls --format '{{.Config.User}} {{.State.Health.Status}}'
```

等待健康后，预期输出包含 `65532:65532 healthy`。再用 curl 创建并访问短链，
重启 Compose 后确认短链仍可访问。完成后运行 `docker compose down`，保留数据卷
供任务 12 的迁移测试，删除 `myurls:pre-hardening`。

- [ ] **步骤 7：提交容器加固**

```sh
git add .dockerignore Dockerfile docker-compose.yaml .env.example logger.go
git diff --cached --check
git commit -m "chore(容器): 加固镜像并升级 Redis 8"
```

---

### 任务 10：重建 CI、E2E 和 GHCR 发布

**文件：**
- 创建：`package.json`
- 创建：`package-lock.json`
- 创建：`playwright.config.js`
- 创建：`tests/e2e/app.spec.js`
- 创建：`tests/integration/redis_test.go`
- 创建：`.github/dependabot.yml`
- 修改：`.github/workflows/go.yml`
- 修改：`.github/workflows/docker_build_push.yml`

- [ ] **步骤 1：编写真实 Redis 集成测试**

`tests/integration/redis_test.go` 使用 `package integration` 和 build tag
`integration`，不导入根目录的 `main` 包；它从
`MYURLS_REDIS_CONN` 建立独立客户端，测试 Ping、NX、TTL、100 并发同键和清理。
没有环境变量时调用 `t.Skip`；CI 必须显式提供变量，不能跳过。

运行无 Redis：`go_docker go test -tags=integration ./tests/integration`

预期：SKIP。启动 Redis 8 并设置环境变量后预期 PASS。

- [ ] **步骤 2：添加固定版本 Playwright**

`package.json` 只包含：

```json
{
  "name": "myurls-e2e",
  "private": true,
  "scripts": { "test:e2e": "playwright test" },
  "devDependencies": { "@playwright/test": "1.62.1" }
}
```

运行 `npm install --package-lock-only` 生成 lockfile。E2E 测试覆盖桌面
1440x900 和移动端 390x844：提交 URL、加载状态、短链结果、复制按钮、错误消息、
无控制台错误、无水平溢出。

创建 `playwright.config.js`，固定 `baseURL` 为 `http://127.0.0.1:8080`，定义
Desktop Chromium 与 Mobile Chromium 两个 project；服务生命周期由本任务步骤 7
的 Compose 命令管理，不在 Playwright 中重复启动。

- [ ] **步骤 3：写入不可变 Actions 引用**

工作流只使用以下 SHA，并保留版本注释：

```yaml
actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
actions/setup-go@b7ad1dad31e06c5925ef5d2fc7ad053ef454303e # v7.0.0
actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
docker/setup-buildx-action@bb05f3f5519dd87d3ba754cc423b652a5edd6d2c # v4.2.0
docker/login-action@dbcb813823bdd20940b903addbd779551569679f # v4.6.0
docker/build-push-action@53b7df96c91f9c12dcc8a07bcb9ccacbed38856a # v7.3.0
```

- [ ] **步骤 4：重建普通 CI**

`go.yml` 对 push/PR 执行：gofmt diff、vet、test、shuffle 20、race、
`go run golang.org/x/vuln/cmd/govulncheck@v1.6.0 ./...`、集成 Redis 8、
多目标构建、Docker smoke、npm ci 和 Playwright。Redis service 使用精确
8.10.0 digest；Go 使用 `go-version: '1.26.5'`。

- [ ] **步骤 5：重建 GHCR 发布**

`docker_build_push.yml` 只响应 `v*` tag 和 `workflow_dispatch`，权限设为
`contents: read`、`packages: write`；登录 `ghcr.io` 使用
`${{ github.actor }}` 和 `${{ secrets.GITHUB_TOKEN }}`；发布
`linux/386,linux/amd64,linux/arm64,linux/ppc64le,linux/arm/v7`，tag 包含版本和
`${{ github.sha }}`，输出 digest，启用 provenance 和 SBOM。

- [ ] **步骤 6：添加 Dependabot**

每周一分别检查 `gomod`、`npm`、`docker`、`github-actions`，目录均为 `/`，
`open-pull-requests-limit: 5`，不配置自动合并。

- [ ] **步骤 7：本地验证工作流资产**

运行：

```sh
npm ci
npx playwright install chromium
docker compose up -d --build
trap 'docker compose down' EXIT
docker compose ps
npm run test:e2e
docker compose config
git diff --check
docker compose down
trap - EXIT
```

预期：启动后 MyUrls 与 Redis 都为 healthy；E2E 两个 project 全部通过；
YAML/Compose 解析通过；无格式错误。即使 E2E 失败，也必须执行
`docker compose down` 清理本任务创建的服务。

- [ ] **步骤 8：提交 CI 与发布**

```sh
git add package.json package-lock.json playwright.config.js tests .github
git diff --cached --check
git commit -m "ci: 增加全量门禁和 GHCR 发布"
```

---

### 任务 11：同步用户与运维文档

**文件：**
- 修改：`README.md`
- 创建：`docs/operations.md`
- 修改：`.env.example`

- [ ] **步骤 1：编写文档契约红灯检查**

运行：

```sh
rg -n 'Go 1\.24|redis:7|docker-compose up|careywong/myurls:latest|127\.0\.0\.1:6379' README.md .env.example
```

预期：README 命中旧 Go、旧镜像、旧 Compose 命令和不可用 Redis 地址。

- [ ] **步骤 2：重写 README 快速路径**

README 必须包含：项目能力、Go 1.25+/toolchain、原有二进制 flag、全部环境变量、
本地 Redis、`docker compose up -d`、`/healthz`、Token/限流示例、GHCR 镜像、
测试命令和 operations 链接。删除不可用的旧 `docker run` 示例和 PPA Redis
安装方式。

- [ ] **步骤 3：编写 operations.md**

固定章节：架构与端口、配置表、日志、健康检查、备份、Redis 7→8 升级、验证、
Redis 8→7 回滚、镜像升级、digest 回滚、故障诊断。备份步骤必须先
`docker compose stop myurls`，再复制 Redis volume；回滚必须恢复升级前备份，
不得复用 Redis 8 写入后的数据。

- [ ] **步骤 4：实际验证文档命令**

按 README 在临时 Compose project name `myurls-doc-test` 下执行启动、health、
创建、跳转、停止；按 operations 完成一次 Redis 数据备份与恢复演练。

预期：所有命令退出 0，恢复后原短链仍可访问。

- [ ] **步骤 5：确认旧文案清零并提交**

运行：

```sh
rg -n 'Go 1\.24|redis:7|docker-compose up|careywong/myurls:latest|127\.0\.0\.1:6379' README.md .env.example
```

预期：无命中。

```sh
git add README.md docs/operations.md .env.example
git diff --cached --check
git commit -m "docs(部署): 更新配置和 Redis 升级回滚指南"
```

---

### 任务 12：全量回归和发布前证据

**文件：**
- 修改：仅修复本任务验证发现的计划内缺陷
- 不创建发布 tag，不推送

- [ ] **步骤 1：验证仓库边界**

```sh
git branch --show-current
git remote get-url origin
git status --short
```

预期：分支为 `codex/myurls-maintenance`，远端为
`https://github.com/keleyaa/MyUrls.git`，没有计划外文件。

- [ ] **步骤 2：运行 Go 全量门禁**

在 Go 1.26.5 容器中运行：

```sh
go_docker sh -c 'test -z "$(gofmt -l $(find . -type f -name "*.go" -not -path "./.git/*"))"'
go_docker go test -count=1 ./...
go_docker go test -shuffle=on -count=20 ./...
go_docker go test -race -count=1 ./...
go_docker go vet ./...
go_docker go run golang.org/x/vuln/cmd/govulncheck@v1.6.0 ./...
go_docker go list -m -u all
```

预期：gofmt 无文件名；测试、race、vet、漏洞检查退出 0；模块无 Update。

- [ ] **步骤 3：运行全部目标构建**

运行 Makefile 默认、Linux amd64/arm64、Darwin amd64/arm64、Windows amd64。
预期：全部归档生成且非空；随后运行 `make clean`，工作树无构建产物。

- [ ] **步骤 4：运行真实 Redis 7 和 8 集成**

分别启动 `redis:7.4.10` 与固定 digest 的 Redis 8.10.0，执行 integration tag
测试和 100 并发同键测试。预期两条版本线均通过，证明协议最低要求未抬高。

- [ ] **步骤 5：运行容器与持久化门禁**

构建最终镜像，检查 User、Healthcheck、只读运行、capabilities、Redis 不暴露；
创建短链后重启全部服务，确认短链仍存在；执行一次备份和回滚恢复。

- [ ] **步骤 6：运行桌面与移动浏览器门禁**

使用 Playwright 执行 1440x900 和 390x844；保存截图到临时目录，检查无空白、
无横向溢出、无控件重叠、无控制台错误、提交与复制路径可用。

- [ ] **步骤 7：检查依赖与外部资源**

```sh
rg -n 'https?://' public
docker history --no-trunc myurls:verify
git diff --check
```

预期：public 不引用外部资源；镜像历史不含 secret；diff 无格式错误。

- [ ] **步骤 8：形成最终状态报告**

记录每条命令、退出码、测试数量、镜像 digest、Redis 版本、浏览器用例数和任何
计划偏差。不得声称 GitHub CI 已通过，除非分支已获授权推送且远端运行完成。

- [ ] **步骤 9：进入分支收尾**

调用 `superpowers:finishing-a-development-branch`，再次运行该技能要求的验证，
向用户提供保留分支、合并、PR 或清理选项；未经选择不执行推送或合并。
