# MyUrls

MyUrls 是一个由 Go 和 Redis 驱动的轻量短链接服务，提供网页界面、`POST /short`
创建接口、`GET /:shortKey` 跳转和依赖 Redis 的 `GET /healthz` 健康检查。

## 界面预览

![MyUrls Luminous Focus 网站界面](output/playwright/myurls-luminous-focus.png)

当前实现默认保持已有接口兼容性：创建接口同时接受表单和 JSON，请求字段仍为
`longUrl`、`shortKey`，HTTP 200 成功响应仍为 `{"Code":1,"ShortUrl":"..."}`，短码继续
区分大小写。服务会拒绝危险的跳转协议和保留短码，并支持可选 Bearer Token、全局
创建限流和请求体大小限制。

容器默认以非 root 用户运行，根文件系统只读并移除 Linux capabilities；Compose
不发布 Redis 宿主机端口，Redis 默认仅在 Compose 网络可达。Docker 管理员或获准加入
该网络的容器仍可访问 Redis。兼容性与安全边界详见
[运维指南](docs/operations.md)。

## 运行要求

- `go.mod` 的 Go 语言版本为 `1.25.0`，建议使用其中固定的 `go1.26.5` toolchain。
- Redis 7.4 或 8.x；仓库 Compose 当前固定 Redis 8.10.0 镜像 digest。
- 容器部署需要 Docker Engine 与 Docker Compose v2。
- 浏览器端到端测试需要 Node.js 24.18.1 和 Chromium。

## Docker Compose 快速启动

在当前仓库中执行：

```sh
cp .env.example .env
# 部署前编辑 .env：至少设置公开域名，并为公网服务设置强 Redis 密码和 API Token。
(
  set +e
  (
    set -eu
    docker compose up -d --wait --wait-timeout 120
    docker compose ps
    curl --fail http://localhost:8080/healthz
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

访问 `http://localhost:8080/`。默认 Redis 数据保存在 `./data/redis`；应用日志和访问日志
均输出到容器标准输出。停止服务：

```sh
docker compose down
```

容器和应用日志统一使用 `Asia/Shanghai`（UTC+8）。访问日志仅保留方法、Gin 路由模板、
状态码和耗时；真实短码显示为 `/:shortKey`，不记录 IP、User-Agent、Query、请求体或
Authorization。成功的 `/healthz` 不写访问日志，失败检查仍会保留。应用日志和访问日志均写入
标准输出，Compose 使用 `json-file` 将每个容器日志限制为单文件 10 MB、最多 3 个文件。业务异常、运行期停止与 panic 恢复日志只记录固定事件，不回显
短码、长链接、Token、Redis 地址、底层错误文本或请求头；无效数值配置和本地健康检查失败
也不会输出原始配置值或网络错误。

升级、备份和恢复前请先阅读[运维指南](docs/operations.md)，不要直接删除 Redis 数据目录。

## 直接连接本地 Redis

若本机已有 Redis，可不使用 Compose。先确认 Redis 可用，再启动应用：

```sh
redis-cli -h localhost -p 6379 ping
go run . -conn localhost:6379 -proto http -domain localhost:8080 -port 8080
```

预期第一条命令返回 `PONG`。如 Redis 开启密码，同时传入 `-password`；生产环境更推荐
通过 `MYURLS_REDIS_PASSWORD` 注入，避免密码留在 shell 历史和进程参数中。

## 创建和访问短链接

表单请求与 JSON 请求均受支持：

```sh
(
  set -eu
  # application/x-www-form-urlencoded
  command -v openssl >/dev/null
  SHORT_KEY="docs-$(openssl rand -hex 8)"
  printf '%s\n' "$SHORT_KEY" | grep -Eq '^docs-[0-9a-f]{16}$'
  curl --fail-with-body http://localhost:8080/short \
    --data-urlencode 'longUrl=https://example.com/docs' \
    --data-urlencode "shortKey=${SHORT_KEY}"

  # application/json；省略 shortKey 时自动生成
  curl --fail-with-body http://localhost:8080/short \
    -H 'Content-Type: application/json' \
    -d '{"longUrl":"https://example.com/guide"}'

  test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
    "http://localhost:8080/${SHORT_KEY}")" = 301
)
```

兼容响应中的业务错误仍可能使用 HTTP 200；调用方必须同时检查响应 JSON 的 `Code`。

### 可选鉴权与限流

`MYURLS_API_TOKEN` 非空时，仅 `POST /short` 需要 Bearer Token；首页、跳转和健康检查
仍可访问。`MYURLS_RATE_LIMIT_RPS=0` 关闭限流：

```sh
export MYURLS_API_TOKEN='replace-with-a-strong-random-secret'
export MYURLS_RATE_LIMIT_RPS=2
export MYURLS_RATE_LIMIT_BURST=4
(
  set +e
  (
    set -eu
    docker compose up -d --wait --wait-timeout 120
    curl --fail-with-body http://localhost:8080/short \
      -H "Authorization: Bearer ${MYURLS_API_TOKEN}" \
      -H 'Content-Type: application/json' \
      -d '{"longUrl":"https://example.com/private"}'
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

## 二进制参数

环境变量在参数解析后应用，因此同名环境变量优先于命令行 flag。

| Flag | 默认值 | 说明 |
| --- | --- | --- |
| `-h` | `false` | 显示帮助并退出 |
| `-port` | `8080` | HTTP 监听端口 |
| `-domain` | `localhost:8080` | 生成短链接时使用的域名，可含端口 |
| `-proto` | `https` | 生成短链接时使用的协议 |
| `-conn` | `localhost:6379` | Redis 地址 |
| `-password` | 空 | Redis 密码 |
| `-healthcheck` | `false` | 请求本机 `/healthz`，按结果退出，不启动服务 |

示例：

```sh
./myurls -healthcheck -port 8080
```

## 环境变量

| 变量 | `.env.example` 默认值 | 作用域 | 说明 |
| --- | --- | --- | --- |
| `MYURLS_PORT` | `8080` | 应用、Compose | HTTP 监听和宿主机映射端口 |
| `MYURLS_DOMAIN` | `example.com` | 应用、Compose | 生成短链接时使用的域名 |
| `MYURLS_PROTO` | `https` | 应用、Compose | 生成短链接时使用的协议 |
| `MYURLS_BASE_URL` | 空 | 应用、Compose | 可选公开短链接基址；非空时优先于 `MYURLS_DOMAIN` 与 `MYURLS_PROTO`，可包含 path prefix |
| `MYURLS_REDIS_CONN` | `myurls-redis:6379` | 应用、Compose | Redis 地址；Compose 内使用服务名 |
| `MYURLS_REDIS_PASSWORD` | 空 | 应用、Compose | Redis 密码；部署时应使用强随机秘密 |
| `MYURLS_REDIS_URL` | 空 | 应用、Compose | 托管 Redis 的 `redis://` / `rediss://` URI；非空时优先于旧地址和密码变量 |
| `MYURLS_REDIS_DATA_PATH` | `./data/redis` | Compose | Redis 宿主机持久化目录 |
| `MYURLS_API_TOKEN` | 空 | 应用、Compose | `POST /short` 的可选 Bearer Token；空值关闭鉴权 |
| `MYURLS_RATE_LIMIT_RPS` | `5` | 应用、Compose | 每秒补充令牌数；`0` 关闭限流 |
| `MYURLS_RATE_LIMIT_BURST` | `10` | 应用、Compose | 限流突发容量 |
| `MYURLS_MAX_BODY_BYTES` | `16384` | 应用、Compose | 创建请求体最大字节数，最小 1024 |
| `MYURLS_READ_HEADER_TIMEOUT` | `5s` | 应用、Compose | HTTP 请求头读取超时 |
| `MYURLS_READ_TIMEOUT` | `10s` | 应用、Compose | HTTP 请求读取超时 |
| `MYURLS_WRITE_TIMEOUT` | `10s` | 应用、Compose | HTTP 响应写入超时 |
| `MYURLS_IDLE_TIMEOUT` | `60s` | 应用、Compose | HTTP keep-alive 空闲超时 |
| `MYURLS_SHUTDOWN_TIMEOUT` | `10s` | 应用、Compose | 应用优雅退出等待时间 |
| `MYURLS_STOP_GRACE_PERIOD` | `20s` | Compose | 容器停止宽限期，必须大于应用退出等待时间 |

所有时长使用 Go duration 格式，例如 `500ms`、`10s`、`2m`。完整配置和故障处理见
[运维指南](docs/operations.md)。

`MYURLS_REDIS_URL` 支持 URI 中的用户名、密码和数据库编号（0–15）。`rediss://`
会启用 TLS。解析失败时程序只返回固定错误，不在错误或日志中回显包含凭据的 URI。

`MYURLS_BASE_URL` 仅接受无凭据、无 query 和 fragment 的绝对 HTTP(S) URL；例如
`https://example.com/links` 会生成 `https://example.com/links/<shortKey>`。

## GHCR 镜像

发布工作流只在扁平的 `v*` Git 标签或手动运行时构建 `ghcr.io/keleyaa/myurls`；标签中
含 `/` 不会触发当前发布规则。它总会发布实际 version 标签和完整 Git commit SHA 标签。
只有推送完整稳定标签 `vX.Y.Z`（例如 `v1.2.3`）时才会把 `latest` 移动到同一 digest；
手动运行和预发布标签（例如 `v1.2.3-rc.1`）不会改变 `latest`。Git 标签含 OCI 非法字符
（例如 `+`）或过长时，工作流会转换 version 标签；必须从 workflow summary 读取实际值。
首次稳定发行成功前，`latest` 不存在。使用前先在仓库 Packages 页面确认目标标签已经存在，例如：

```sh
docker pull ghcr.io/keleyaa/myurls:v1.2.3
docker pull ghcr.io/keleyaa/myurls:0123456789abcdef0123456789abcdef01234567
# 仅在完整稳定发行成功后可用：
docker pull ghcr.io/keleyaa/myurls:latest
```

仓库的 `docker-compose.yaml` 默认使用本地 `build`。使用 GHCR 镜像部署时，请按
[镜像升级](docs/operations.md#镜像升级)中的 override 与 `--no-build` 流程操作。

## 构建与测试

```sh
# 构建当前平台二进制到 build/myurls
make

# Go 格式、vet 和单元测试
make verify

# 真实 Redis 集成测试；需先启动 Redis
MYURLS_REDIS_CONN=localhost:6379 go test -tags=integration -count=1 ./tests/integration

# 浏览器端到端测试；需先启动 Compose 服务
npm ci
npx playwright install chromium
npm run test:e2e
```

GitHub Actions 会执行格式、vet、单元/乱序/race/漏洞、真实 Redis、跨平台构建、容器
边界与桌面/移动 E2E 门禁。只有远端工作流实际完成后，才能将其结果视为 CI 通过。

## 维护者与许可

原作者与维护者：[@CareyWang](https://github.com/CareyWang)。欢迎提交 PR。

MIT © 2024 CareyWang。完整文本见 [LICENSE](LICENSE)。
