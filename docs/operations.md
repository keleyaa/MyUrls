# MyUrls 运维指南

本文以仓库中的 `docker-compose.yaml` 为基准。所有示例都应在仓库根目录执行；生产
环境运行前先检查 `docker compose config`，并把示例域名、密码、Token、镜像标签和
备份路径替换为实际值。

命令前提：POSIX shell、Docker、Docker Compose v2、`curl`、`jq`、`tar`、`openssl`、
`awk`、`grep`、`find`、`mv`、`mktemp`、`cmp`、`rm`。`jq` 用于精确验证兼容响应；`tar` 与 `openssl` 用于完整归档和 SHA-256
校验。所有启动命令均有 120 秒 readiness 上限，不使用固定 sleep；若失败，应停止后续
步骤并检查 `docker compose ps --all` 与相关服务日志。

## 架构与端口

```text
客户端 ── HTTP/HTTPS ──> myurls:8080 ── Compose 内部网络 ──> myurls-redis:6379
```

Compose 只将 `myurls` 的 `MYURLS_PORT` 映射到宿主机；Redis 未发布宿主机端口，默认
仅同一 Compose 网络中的容器可访问。Docker 管理员或获准加入该网络的容器仍可访问
Redis。若前置反向代理终止 TLS，应让 `MYURLS_DOMAIN` 和 `MYURLS_PROTO` 与用户实际
访问地址一致；若公开地址包含固定 path prefix，可改设完整的 `MYURLS_BASE_URL`，它优先于这两个旧变量。

应用镜像最终层基于 `scratch`，没有 shell，以 UID/GID `65532:65532` 运行。Compose
同时启用只读根文件系统、`no-new-privileges` 并移除全部 Linux capabilities；应用不需要
可写日志目录。Redis 数据通过 `MYURLS_REDIS_DATA_PATH` 绑定到容器 `/data`。

## 配置表

| 变量 | 默认值 | 读取方 | 说明 |
| --- | --- | --- | --- |
| `MYURLS_PORT` | `8080` | 应用、Compose | HTTP 监听与宿主机映射端口 |
| `MYURLS_DOMAIN` | `example.com` | 应用、Compose | 返回短链接中的域名，可包含端口 |
| `MYURLS_PROTO` | `https` | 应用、Compose | 返回短链接中的协议 |
| `MYURLS_BASE_URL` | 空 | 应用、Compose | 可选完整公开基址；仅允许 HTTP(S)，可包含 path prefix，非空时优先于域名和协议 |
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

应用业务日志和访问日志均写入标准输出；Redis 日志也写入标准输出。镜像和应用日志统一使用
`Asia/Shanghai`（UTC+8），时间字段采用带 `+08:00` 偏移的 RFC 3339 格式。

访问日志只记录请求方法、Gin 路由模板、状态码和耗时。真实短码统一显示为
`/:shortKey`，未匹配地址显示为 `unmatched`；不会记录客户端 IP、User-Agent、Query、
请求体或 Authorization。成功的 `/healthz` 不写访问日志，HTTP 4xx/5xx 健康检查仍会
保留，因此健康状态变化可诊断而不会每 30 秒制造一条成功记录。

业务异常、运行期停止与 panic 恢复日志同样只记录固定事件，不输出真实短码、长链接、
Token、Redis 地址、底层错误文本、请求行或请求头。Gin 默认的恢复请求转储已禁用，避免
异常连接时将 Query 或非 Authorization 请求头写入标准错误。无效数值配置和本地健康检查
失败同样不回显原始配置值或网络错误。

Compose 的 `json-file` 日志轮转限制为单文件 10 MB、最多 3 个文件，覆盖应用、访问日志和
Redis 的标准输出。若需要更长保留或集中检索，应配置 Docker logging driver 或平台日志采集器。

```sh
docker compose logs --tail=200 myurls
docker compose logs --tail=200 myurls-redis
docker compose logs --follow --since=10m
```

不要把完整请求授权头、Redis 密码、API Token、真实短码、长链接或 `.env` 内容粘贴到
工单。通过 `docker compose logs` 或平台日志采集器查看日志；不要依赖或手工修改容器文件系统中的日志文件。

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

以下冷备份流程适用于当前 bind mount。它在任何停止操作前确认数据源就是 Compose
实际挂载的 `/data` 源；随后停止应用并确认非 running，在 Redis 仍运行时同步执行
`SAVE`、记录版本及 RDB/AOF 配置，最后以 60 秒上限停止 Redis 并确认非 running。
仅非 running 不足以证明干净停机：应用与 Redis 还必须是退出码 0、未被 OOM kill 且
没有 runtime error。整个数据目录会被归档，因此 RDB、AOF 和 `appendonlydir` 都会
保留；checksum 必须严格等于按固定文件顺序重新生成的两行 canonical 清单。

```sh
(
  set +e
  # 必须与 .env 中的 MYURLS_REDIS_DATA_PATH 一致；赋值只在本子 shell 生效。
  MYURLS_REDIS_DATA_PATH="${MYURLS_REDIS_DATA_PATH:-./data/redis}"
  BACKUP_DIR="./backups/myurls-$(date +%Y%m%d-%H%M%S)"
  export MYURLS_REDIS_DATA_PATH BACKUP_DIR
  (
    set -eu
    for tool in docker jq tar openssl awk grep mktemp cmp rm; do command -v "$tool" >/dev/null; done
    test -d "$MYURLS_REDIS_DATA_PATH"
    test ! -e "$BACKUP_DIR"

    data_source="$(cd "$MYURLS_REDIS_DATA_PATH" && pwd -P)"
    compose_source="$(docker compose config --format json |
      jq -er '.services["myurls-redis"].volumes[] | select(.target == "/data") | .source')"
    test "$data_source" = "$compose_source"

    redis_id="$(docker compose ps --quiet myurls-redis)"
    test -n "$redis_id"
    test "$(docker inspect --format '{{.State.Running}}' "$redis_id")" = true
    mkdir -p "$(dirname "$BACKUP_DIR")"
    mkdir "$BACKUP_DIR"

    docker compose stop --timeout 60 myurls
    app_id="$(docker compose ps --all --quiet myurls)"
    test -n "$app_id"
    test "$(docker inspect --format '{{.State.Running}}' "$app_id")" = false
    test "$(docker inspect --format '{{.State.ExitCode}}' "$app_id")" = 0
    test "$(docker inspect --format '{{.State.OOMKilled}}' "$app_id")" = false
    test -z "$(docker inspect --format '{{.State.Error}}' "$app_id")"

    docker compose exec -T myurls-redis sh -eu -c '
      if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
      redis-cli SAVE >/dev/null
      version="$(redis-cli --raw INFO server | sed -n "s/^redis_version://p" | tr -d "\r")"
      save="$(redis-cli --raw CONFIG GET save | tail -n 1 | tr -d "\r")"
      appendonly="$(redis-cli --raw CONFIG GET appendonly | tail -n 1 | tr -d "\r")"
      test -n "$version"
      printf "redis_version=%s\nredis_major=%s\nsave=%s\nappendonly=%s\n" \
        "$version" "${version%%.*}" "$save" "$appendonly"
    ' > "$BACKUP_DIR/redis-manifest.env"

    docker compose stop --timeout 60 myurls-redis
    redis_id="$(docker compose ps --all --quiet myurls-redis)"
    test -n "$redis_id"
    test "$(docker inspect --format '{{.State.Running}}' "$redis_id")" = false
    test "$(docker inspect --format '{{.State.ExitCode}}' "$redis_id")" = 0
    test "$(docker inspect --format '{{.State.OOMKilled}}' "$redis_id")" = false
    test -z "$(docker inspect --format '{{.State.Error}}' "$redis_id")"

    tar -C "$MYURLS_REDIS_DATA_PATH" -cpf "$BACKUP_DIR/redis-data.tar" .
    (
      set -eu
      cd "$BACKUP_DIR"
      openssl dgst -sha256 -r redis-manifest.env redis-data.tar > SHA256SUMS
      checksum_actual="$(mktemp "${TMPDIR:-/tmp}/myurls-checksum.XXXXXX")"
      trap 'rm -f "$checksum_actual"' EXIT
      trap 'trap - EXIT HUP INT TERM; rm -f "$checksum_actual"; exit 1' HUP INT TERM
      openssl dgst -sha256 -r redis-manifest.env redis-data.tar > "$checksum_actual"
      cmp -s SHA256SUMS "$checksum_actual"
      awk '
        NR == 1 {
          if (length($1) != 64 || $1 ~ /[^0-9a-f]/ || $2 != "*redis-manifest.env") exit 1
        }
        NR == 2 {
          if (length($1) != 64 || $1 ~ /[^0-9a-f]/ || $2 != "*redis-data.tar") exit 1
        }
        NR > 2 { exit 1 }
        END { if (NR != 2) exit 1 }
      ' SHA256SUMS
      rm -f "$checksum_actual"
      trap - EXIT HUP INT TERM
    )
  )
  backup_status=$?

  if [ "$backup_status" -ne 0 ]; then
    printf '%s\n' '冷备份失败：不得重启、切换数据路径或继续升级。先检查当前状态和日志。' >&2
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  else
    printf 'cold_backup=%s\n' "$BACKUP_DIR"
    printf '%s\n' '冷备份完成；应用和 Redis 保持停止。'
  fi
)
```

若数据路径错误、停止失败、归档失败或校验不匹配，内层 `set -eu` 会中断，外层不会
执行启动块，也不会改变 Compose 数据路径。失败发生在停止之后时，服务应保持停止，
修正原因并重新完成整块备份后才能启动。把备份复制到另一台受控主机，记录时间和当前
Redis 镜像 digest，并定期做恢复演练。

确认 `cold_backup=` 路径已异地保存后，才可恢复原服务。Redis major 变更不得在该
数据目录上直接执行；请遵循下一节的隔离恢复流程。

```sh
(
  set +e
  (
    set -eu
    docker compose up -d --wait --wait-timeout 120
    curl --fail --silent --show-error http://localhost:8080/healthz
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

## Redis major 版本边界

当前支持路径是不跨 Redis major 原地升级或降级：先在新环境完成恢复演练，再切换流量。
不要让任一 major 直接打开另一 major 已写入的数据目录，也不要承诺 Redis 8→7 的原地回滚。

### 隔离恢复演练

以下流程只解压到全新的数据目录，并用独立 Compose project 与非生产端口启动。执行前保持
生产应用和 Redis 停止；`BACKUP_DIR` 必须是上一步生成且已异地保存的冷备份目录。为目标
major 准备一份仅用于隔离演练的 Compose 文件副本：将其中 `myurls-redis.image` 固定为目标
Redis major 的已验证 digest，并将 `myurls.ports` 替换为
`127.0.0.1:${MYURLS_PORT}:${MYURLS_PORT}`；且不要修改生产的 `docker-compose.yaml`。开始前核对
`redis-manifest.env` 中的 `redis_major`，并确认目标 major 与恢复策略相符。绝不能解压、移动
或覆盖生产的 `MYURLS_REDIS_DATA_PATH`。

```sh
(
  set -eu
  BACKUP_DIR='/absolute/path/to/cold-backup'
  RESTORE_ROOT='/absolute/path/to/isolated-restore'
  RESTORE_DATA_PATH="$RESTORE_ROOT/redis-data"
  RESTORE_PORT=8081
  RESTORE_COMPOSE='/absolute/path/to/compose.redis-target.yaml'
  TARGET_REDIS_MAJOR=8 # 填入计划切换的 Redis major。
  RESTORE_REDIS_PASSWORD='' # 若冷备份启用了 Redis 密码，在此处设置相同值。

  test -f "$BACKUP_DIR/redis-manifest.env"
  test -f "$BACKUP_DIR/redis-data.tar"
  test -f "$BACKUP_DIR/SHA256SUMS"
  mkdir -p "$RESTORE_ROOT"
  test ! -e "$RESTORE_DATA_PATH"

  (
    cd "$BACKUP_DIR"
    openssl dgst -sha256 -r redis-manifest.env redis-data.tar | cmp -s SHA256SUMS -
  )
  grep -Eq '^redis_major=[0-9]+$' "$BACKUP_DIR/redis-manifest.env"

  mkdir "$RESTORE_DATA_PATH"
  tar -C "$RESTORE_DATA_PATH" -xpf "$BACKUP_DIR/redis-data.tar"

  MYURLS_REDIS_DATA_PATH="$RESTORE_DATA_PATH" \
  MYURLS_PORT="$RESTORE_PORT" \
  MYURLS_REDIS_PASSWORD="$RESTORE_REDIS_PASSWORD" \
  MYURLS_REDIS_CONN='myurls-redis:6379' \
  MYURLS_REDIS_URL='' \
  docker compose -f docker-compose.yaml -f "$RESTORE_COMPOSE" -p myurls-restore up -d --wait --wait-timeout 120

  running_redis_version="$(docker compose -f docker-compose.yaml -f "$RESTORE_COMPOSE" -p myurls-restore \
    exec -T myurls-redis sh -ec '\
      if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
      redis-cli --raw INFO server
    ' | sed -n 's/^redis_version://p' | tr -d '\r')"
  test "${running_redis_version%%.*}" = "$TARGET_REDIS_MAJOR"

  curl --fail --silent --show-error "http://localhost:${RESTORE_PORT}/healthz"
  # 用一个新短码调用 POST /short，并确认一条升级前已知短码仍返回 301。
)
```

若 checksum、解压、启动或任一验证失败，执行
`docker compose -f docker-compose.yaml -f "$RESTORE_COMPOSE" -p myurls-restore down`，保留
生产数据目录不变并排查失败原因。仅在隔离环境的 `/healthz`、创建和旧短链接 301 都通过后，
才可在维护窗口规划流量切换；切换前再次确认已保留可恢复的冷备份。

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
(
  set +e
  (
    set -eu
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml pull myurls
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml \
      up -d --no-build --wait --wait-timeout 120 myurls
    curl --fail --silent --show-error http://localhost:8080/healthz
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml ps --all
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml logs --tail=200 myurls
    false
  fi
)
```

发布 Git 标签必须是以 `v` 开头的扁平标签，例如 `v1.2.3`；含 `/` 不会触发当前
`v*` 发布规则。即使工作流被触发，Git 标签含 OCI 非法字符（例如 `+`）或超过长度
限制时也会被转换。必须从 workflow summary 复制实际 version 标签；工作流同时发布
完整 Git commit SHA 标签。只有推送完整稳定标签 `vX.Y.Z` 时，`latest` 才移动到同一
digest；手动运行和预发布标签不会改变它。首次稳定发行成功前不要引用 `latest`。

## digest 回滚

发布后记录 `docker image inspect` 或 GHCR 构建摘要中的不可变 digest。镜像回滚应使用
`name@sha256:...`，避免同名标签漂移：

```sh
export MYURLS_IMAGE='ghcr.io/keleyaa/myurls@sha256:<已验证的完整 digest>'
(
  set +e
  (
    set -eu
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml pull myurls
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml \
      up -d --no-build --wait --wait-timeout 120 myurls
    curl --fail --silent --show-error http://localhost:8080/healthz
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml ps --all
    docker compose -f docker-compose.yaml -f compose.ghcr.yaml logs --tail=200 myurls
    false
  fi
)
```

应用镜像回滚不应顺带回滚 Redis 数据。若故障来自 Redis major 变更，必须遵循
[Redis major 版本边界](#redis-major-版本边界)中的隔离恢复流程，并使用升级前的冷备份。

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
