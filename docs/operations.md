# MyUrls 运维指南

本文以仓库中的 `docker-compose.yaml` 为基准。所有示例都应在仓库根目录执行；生产
环境运行前先检查 `docker compose config`，并把示例域名、密码、Token、镜像标签和
备份路径替换为实际值。

## 架构与端口

```text
客户端 ── HTTP/HTTPS ──> myurls:8080 ── Compose 内部网络 ──> myurls-redis:6379
```

Compose 只将 `myurls` 的 `MYURLS_PORT` 映射到宿主机；Redis 没有 `ports` 配置，不能
从宿主机或公网直接访问。若前置反向代理终止 TLS，应让 `MYURLS_DOMAIN` 和
`MYURLS_PROTO` 与用户实际访问地址一致。

应用镜像最终层基于 `scratch`，没有 shell，以 UID/GID `65532:65532` 运行。Compose
同时启用只读根文件系统、`no-new-privileges` 并移除全部 Linux capabilities；只有日志
volume 可写。Redis 数据通过 `MYURLS_REDIS_DATA_PATH` 绑定到容器 `/data`。

## 配置表

| 变量 | 默认值 | 读取方 | 说明 |
| --- | --- | --- | --- |
| `MYURLS_PORT` | `8080` | 应用、Compose | HTTP 监听与宿主机映射端口 |
| `MYURLS_DOMAIN` | `example.com` | 应用、Compose | 返回短链接中的域名，可包含端口 |
| `MYURLS_PROTO` | `https` | 应用、Compose | 返回短链接中的协议 |
| `MYURLS_REDIS_CONN` | `myurls-redis:6379` | 应用、Compose | Redis 地址；容器内应使用服务名 |
| `MYURLS_REDIS_PASSWORD` | 空 | 应用、Redis、Compose | Redis 密码；生产环境必须妥善保管 |
| `MYURLS_REDIS_DATA_PATH` | `./data/redis` | Compose | 宿主机 Redis 持久化目录 |
| `MYURLS_API_TOKEN` | 空 | 应用、Compose | 创建接口 Bearer Token；空值关闭鉴权 |
| `MYURLS_RATE_LIMIT_RPS` | `5` | 应用、Compose | 创建接口每秒令牌数；`0` 关闭限流 |
| `MYURLS_RATE_LIMIT_BURST` | `10` | 应用、Compose | 创建接口突发容量 |
| `MYURLS_MAX_BODY_BYTES` | `16384` | 应用、Compose | 创建请求体上限，不能小于 1024 |
| `MYURLS_READ_HEADER_TIMEOUT` | `5s` | 应用、Compose | 请求头读取超时 |
| `MYURLS_READ_TIMEOUT` | `10s` | 应用、Compose | 请求读取超时 |
| `MYURLS_WRITE_TIMEOUT` | `10s` | 应用、Compose | 响应写入超时 |
| `MYURLS_IDLE_TIMEOUT` | `60s` | 应用、Compose | keep-alive 空闲超时 |
| `MYURLS_SHUTDOWN_TIMEOUT` | `10s` | 应用、Compose | 应用收到停止信号后的退出期限 |
| `MYURLS_STOP_GRACE_PERIOD` | `20s` | Compose | 容器停止宽限期，必须大于应用退出期限 |

`MYURLS_STOP_GRACE_PERIOD` 必须大于 `MYURLS_SHUTDOWN_TIMEOUT`，否则 Docker 可能在
应用清理 Redis、日志等资源前发送强制终止信号。

Redis 密码和 API Token 都是秘密。当前 Compose 模型通过容器环境变量传入密码，并在
Redis 启动参数中启用 `requirepass`；拥有 Docker 管理权限的用户可能通过容器配置或
进程信息读取它。应限制 Docker socket 与主机访问权限，不把 `.env` 提交到仓库；更高
安全要求下应迁移到支持文件型 secret 的编排平台。

## 日志

应用访问日志写入 Compose 管理的 `myurls-logs` volume；容器运行状态和启动错误通过
标准输出查看。Redis 日志写入其标准输出。

```sh
docker compose logs --tail=200 myurls
docker compose logs --tail=200 myurls-redis
docker compose logs --follow --since=10m
```

不要把完整请求授权头、Redis 密码或 `.env` 内容粘贴到工单。检查日志 volume 位置时可
使用 `docker volume inspect`，但应通过备份策略而不是手工修改该目录。

## 健康检查

`GET /healthz` 会实际 Ping Redis：HTTP 200 和 `{"status":"ok"}` 表示应用及 Redis
均可用；HTTP 503 仅返回通用不可用状态，不泄漏内部错误。

```sh
curl --fail --silent --show-error http://localhost:8080/healthz
docker compose ps
docker compose exec myurls-redis sh -ec '
  if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
  exec redis-cli PING
'
```

应用容器没有 shell；其镜像健康检查直接运行 `/app/myurls -healthcheck`。不要使用
`docker compose exec myurls sh` 诊断，应查看健康状态、日志和 HTTP 响应。

## 备份

以下冷备份流程适用于当前 bind mount。必须先停止应用，避免备份窗口继续产生写入；
再停止 Redis 并等待其退出，确保 RDB/AOF 文件处于一致状态，最后复制配置的数据目录。

```sh
# 必须与 .env 中的 MYURLS_REDIS_DATA_PATH 一致。
export MYURLS_REDIS_DATA_PATH=./data/redis
export BACKUP_DIR="./backups/myurls-$(date +%Y%m%d-%H%M%S)"

docker compose stop myurls
docker compose stop myurls-redis
mkdir -p "$BACKUP_DIR"
cp -a "$MYURLS_REDIS_DATA_PATH" "$BACKUP_DIR/redis-data"
test -d "$BACKUP_DIR/redis-data"
```

不要在 Redis 仍运行时直接复制 `/data`。备份完成后可原样启动：

```sh
docker compose up -d
docker compose ps
```

将备份复制到另一台受控主机，记录备份时间、当前 Redis 镜像 digest 和校验和，并定期
做恢复演练。若启用了 AOF，备份必须同时保留 `appendonlydir`；默认配置主要使用 RDB。

## Redis 7→8 升级

1. 在 Redis 7 仍运行时记录版本和持久化配置，并创建一个可验证的测试短链。
2. 严格按“备份”章节停止 `myurls`、停止 Redis，再复制升级前数据目录。
3. 将 `docker-compose.yaml` 中 Redis 镜像改为经验证的 Redis 8 版本及固定 digest。
4. 只启动 Redis，检查数据加载无误后再启动应用。

```sh
docker compose pull myurls-redis
docker compose up -d myurls-redis
docker compose logs --tail=200 myurls-redis

docker compose exec myurls-redis sh -ec '
  if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
  redis-cli INFO server | grep "^redis_version:"
  redis-cli CONFIG GET save
  redis-cli CONFIG GET appendonly
  redis-cli PING
'

docker compose up -d myurls
```

不要仅凭容器处于 running 判断升级成功。必须检查 Redis 日志中没有 RDB/AOF 加载错误，
并执行下一章的全部验证。升级确认前保留升级前备份且设为只读。

## 验证

```sh
# 1. 容器健康与 Redis 协议
docker compose ps
curl --fail --silent --show-error http://localhost:8080/healthz
docker compose exec myurls-redis sh -ec '
  if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
  redis-cli PING
  redis-cli LASTSAVE
'

# 2. 创建并检查跳转；替换为不会与现有短码冲突的值。
# 若 .env 启用了鉴权，先通过安全的秘密注入方式将同一 Token 导出到当前 shell。
curl --fail-with-body http://localhost:8080/short \
  -H "Authorization: Bearer ${MYURLS_API_TOKEN:-}" \
  -H 'Content-Type: application/json' \
  -d '{"longUrl":"https://example.com/upgrade-check","shortKey":"upgrade-check"}'
test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  http://localhost:8080/upgrade-check)" = 301

# 3. 重启后再次确认持久化数据和健康状态
docker compose restart myurls-redis myurls
docker compose ps
curl --fail --silent --show-error http://localhost:8080/healthz
test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
  http://localhost:8080/upgrade-check)" = 301
```

若 `CONFIG GET appendonly` 返回 `yes`，还要确认 Redis 日志无 AOF 截断或重放错误；无论
是否启用 AOF，都要确认 RDB 配置、`LASTSAVE`、重启后旧短链和新短链均可访问。

## Redis 8→7 回滚

Redis 7 不保证能够读取 Redis 8 写出的持久化文件。回滚必须恢复升级前的 Redis 7 冷
备份，绝不能让 Redis 7 复用或打开 Redis 8 已写入的数据目录；升级后的新增数据需要先
通过独立、经过验证的迁移流程处理，不能直接复制持久化文件。

```sh
export MYURLS_REDIS_DATA_PATH=./data/redis
export PRE_UPGRADE_BACKUP=./backups/myurls-before-redis8/redis-data
export REDIS8_DATA_HOLD="./data/redis8-hold-$(date +%Y%m%d-%H%M%S)"

docker compose stop myurls
docker compose stop myurls-redis
test -d "$PRE_UPGRADE_BACKUP"
mv "$MYURLS_REDIS_DATA_PATH" "$REDIS8_DATA_HOLD"
mkdir -p "$MYURLS_REDIS_DATA_PATH"
cp -a "$PRE_UPGRADE_BACKUP/." "$MYURLS_REDIS_DATA_PATH/"
```

然后把 Compose 中的 Redis 镜像改回已记录的 Redis 7 固定 digest，只启动 Redis 并检查
日志、版本、RDB/AOF 加载和 `PING`，确认无误后再启动应用：

```sh
docker compose up -d myurls-redis
docker compose logs --tail=200 myurls-redis
docker compose exec myurls-redis sh -ec '
  if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
  redis-cli INFO server | grep "^redis_version:"
  redis-cli PING
'
docker compose up -d myurls
curl --fail http://localhost:8080/healthz
```

回滚验证完成前保留 `REDIS8_DATA_HOLD`，不要覆盖或删除它。

## 镜像升级

仓库 Compose 默认包含 `build: .`。为避免升级时意外重建本地源码，创建一个不提交的
override 文件指定 GHCR 镜像，并始终使用 `--no-build`：

```sh
cat > compose.ghcr.yaml <<'YAML'
services:
  myurls:
    image: ${MYURLS_IMAGE:?set MYURLS_IMAGE to an immutable release image}
YAML

# 使用已在 GHCR Packages 中确认存在的 v* 版本标签或完整 commit SHA 标签。
export MYURLS_IMAGE=ghcr.io/keleyaa/myurls:v1.2.3
docker compose -f docker-compose.yaml -f compose.ghcr.yaml pull myurls
docker compose -f docker-compose.yaml -f compose.ghcr.yaml up -d --no-build myurls
docker compose -f docker-compose.yaml -f compose.ghcr.yaml ps
```

发布标签必须是以 `v` 开头的扁平标签，例如 `v1.2.3`；Git ref 名含 `/` 时不能直接
作为 OCI 镜像标签。工作流也发布完整 Git commit SHA 标签，不发布 `latest`。

## digest 回滚

发布后记录 `docker image inspect` 或 GHCR 构建摘要中的不可变 digest。镜像回滚应使用
`name@sha256:...`，避免同名标签漂移：

```sh
export MYURLS_IMAGE='ghcr.io/keleyaa/myurls@sha256:<已验证的完整 digest>'
docker compose -f docker-compose.yaml -f compose.ghcr.yaml pull myurls
docker compose -f docker-compose.yaml -f compose.ghcr.yaml up -d --no-build myurls
curl --fail http://localhost:8080/healthz
```

应用镜像回滚不应顺带回滚 Redis 数据。若故障来自 Redis 升级，必须遵循“Redis 8→7
回滚”章节并恢复对应版本的升级前备份。

## 故障诊断

| 现象 | 检查 | 处理 |
| --- | --- | --- |
| `/healthz` 返回 503 | `docker compose logs myurls-redis myurls`、Redis `PING` | 核对服务名、密码、数据权限和 Redis 健康状态 |
| 应用反复退出 | 应用日志中的 `redis ping` 或配置解析错误 | 修正 `.env`；所有 timeout 必须大于 0 |
| 创建接口返回 401 | `MYURLS_API_TOKEN` 与 Authorization 头 | 使用严格的 `Bearer <token>` 格式，不在日志中打印 Token |
| 创建接口返回 429 | RPS 和 burst 配置 | 调整容量，或显式将 RPS 设为 `0` 关闭限流 |
| 创建响应 HTTP 200 但失败 | JSON 中的 `Code`、`Message` | 按兼容业务码处理，不只判断 HTTP 状态 |
| Redis 8 无法加载数据 | Redis 日志中的 RDB/AOF 错误 | 停止写入，保留现场，从升级前冷备份恢复 |
| Redis 7 回滚启动失败 | 是否误用了 Redis 8 写出的目录 | 立即停止 Redis 7，改用升级前 Redis 7 备份 |
| `docker compose exec myurls sh` 失败 | 镜像为 `scratch`、无 shell | 使用日志、健康接口和 `docker inspect` 诊断 |
| 容器被强制终止 | 两个 shutdown 配置值 | 保证 `MYURLS_STOP_GRACE_PERIOD` 大于 `MYURLS_SHUTDOWN_TIMEOUT` |
| Redis 数据目录权限错误 | bind mount 路径与目录所有权 | 停止服务后修正宿主机目录权限，再重启并验证 |

最后收集以下无秘密信息：`docker compose ps`、相关日志尾部、镜像 digest、Redis 版本、
健康检查状态和最近一次可恢复备份时间。GitHub CI 或 GHCR 发布只有在远端工作流实际
运行并成功后才能声称通过；本地检查不能替代远端发布证据。
