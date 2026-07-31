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
访问地址一致。

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

如果只是普通备份而不是立即升级，确认 `cold_backup=` 路径已异地保存后，再用独立块
恢复服务；立即升级 Redis 时不要执行此块，保持维护窗口无写入，直到 Redis 8 及升级前
旧短码验证成功后才启动应用。

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

## Redis 7→8 升级

先在 Redis 7 仍运行时创建唯一的升级前短码，并精确确认兼容响应和跳转。
`openssl rand -hex 8` 生成 8 字节随机后缀，输出只包含短码允许的十六进制字符。记录
第一块打印的 `pre7_key=`，在后续占位符中使用同一个值。

```sh
(
  set +e
  (
    set -eu
    PRE7_KEY="pre7-$(openssl rand -hex 8)"
    printf '%s\n' "$PRE7_KEY" | grep -Eq '^pre7-[0-9a-f]{16}$'
    redis_version="$(docker compose exec -T myurls-redis sh -eu -c '
      if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
      redis-cli --raw INFO server | sed -n "s/^redis_version://p" | tr -d "\r"
    ')"
    case "$redis_version" in 7.*) ;; *) false ;; esac

    pre7_payload="$(jq -nc --arg key "$PRE7_KEY" \
      '{longUrl:"https://example.com/pre-redis8",shortKey:$key}')"
    pre7_response="$(curl --fail-with-body --silent --show-error \
      http://localhost:8080/short \
      -H "Authorization: Bearer ${MYURLS_API_TOKEN:-}" \
      -H 'Content-Type: application/json' -d "$pre7_payload")"
    printf '%s\n' "$pre7_response" |
      jq -e '.Code == 1 and (.ShortUrl | type == "string" and length > 0)' >/dev/null
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${PRE7_KEY}")" = 301
    printf 'pre7_key=%s\n' "$PRE7_KEY"
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

接着按“备份”章节执行完整冷备份，记录其 `cold_backup=` 输出且不要执行普通备份的
重启块。把下面 `PRE7_KEY` 和 `BACKUP_DIR` 占位符替换成记录的值，并确认 manifest 包含
`redis_major=7`。将 `docker-compose.yaml` 中 Redis 镜像改为经验证的 Redis 8 版本和
固定 digest 后，只启动 Redis 并等待健康；确认升级前旧短码数据存在后才启动应用：

```sh
(
  set +e
  (
    set -eu
    PRE7_KEY='<pre7_key 输出值>'
    BACKUP_DIR='./backups/<成功冷备份目录>'
    printf '%s\n' "$PRE7_KEY" | grep -Eq '^pre7-[0-9a-f]{16}$'
    grep -qx 'redis_major=7' "$BACKUP_DIR/redis-manifest.env"
    docker compose pull myurls-redis
    docker compose up -d --wait --wait-timeout 120 myurls-redis
    docker compose logs --tail=200 myurls-redis
    docker compose exec -T -e PRE7_KEY="$PRE7_KEY" myurls-redis sh -eu -c '
      if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
      version="$(redis-cli --raw INFO server | sed -n "s/^redis_version://p" | tr -d "\r")"
      case "$version" in 8.*) ;; *) false ;; esac
      redis-cli CONFIG GET save
      redis-cli CONFIG GET appendonly
      redis-cli PING | grep -qx PONG
      test "$(redis-cli --raw EXISTS "$PRE7_KEY")" = 1
    '
    docker compose up -d --wait --wait-timeout 120 myurls
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${PRE7_KEY}")" = 301
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

不要仅凭容器处于 running 判断升级成功。必须检查 Redis 日志中没有 RDB/AOF 加载错误，
且 Redis 7 创建的 `PRE7_KEY` 在 Redis 8 上仍返回 301。升级确认前保留升级前备份、
`SHA256SUMS` 和 manifest，并将它们设为只读。

## 验证

```sh
(
  set +e
  (
    set -eu
    PRE7_KEY='<pre7_key 输出值>'
    printf '%s\n' "$PRE7_KEY" | grep -Eq '^pre7-[0-9a-f]{16}$'
    UPGRADE_KEY="upgrade-$(openssl rand -hex 8)"
    printf '%s\n' "$UPGRADE_KEY" | grep -Eq '^upgrade-[0-9a-f]{16}$'
    docker compose ps
    curl --fail --silent --show-error http://localhost:8080/healthz
    docker compose exec -T myurls-redis sh -eu -c '
      if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
      redis-cli PING | grep -qx PONG
      redis-cli LASTSAVE
    '

    upgrade_payload="$(jq -nc --arg key "$UPGRADE_KEY" \
      '{longUrl:"https://example.com/upgrade-check",shortKey:$key}')"
    upgrade_response="$(curl --fail-with-body --silent --show-error \
      http://localhost:8080/short \
      -H "Authorization: Bearer ${MYURLS_API_TOKEN:-}" \
      -H 'Content-Type: application/json' -d "$upgrade_payload")"
    printf '%s\n' "$upgrade_response" |
      jq -e '.Code == 1 and (.ShortUrl | type == "string" and length > 0)' >/dev/null
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${UPGRADE_KEY}")" = 301
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${PRE7_KEY}")" = 301

    docker compose restart myurls-redis myurls
    docker compose up -d --wait --wait-timeout 120
    curl --fail --silent --show-error http://localhost:8080/healthz
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${UPGRADE_KEY}")" = 301
    test "$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      "http://localhost:8080/${PRE7_KEY}")" = 301
  )
  block_status=$?
  if [ "$block_status" -ne 0 ]; then
    docker compose ps --all
    docker compose logs --tail=200 myurls myurls-redis
    false
  fi
)
```

若 `CONFIG GET appendonly` 返回 `yes`，还要确认 Redis 日志无 AOF 截断或重放错误；无论
是否启用 AOF，都要确认 RDB 配置、`LASTSAVE`、重启后旧短链和新短链均可访问。

## Redis 8→7 回滚

Redis 7 不保证能够读取 Redis 8 写出的持久化文件。回滚必须恢复升级前的 Redis 7 冷
备份，绝不能让 Redis 7 复用或打开 Redis 8 已写入的数据目录；升级后的新增数据需要先
通过独立、经过验证的迁移流程处理，不能直接复制持久化文件。先把 Compose Redis 镜像
改回已记录的 Redis 7 固定 digest；以下流程会确认目标镜像标签和备份 manifest 都是 7。
即使配置路径以 `/` 结尾，也会先与 Compose mount source 核对并规范为绝对无尾斜杠
路径，之后才派生同父目录的 hold 和 staging。

```sh
(
  set +e
  (
    set -eu
    MYURLS_REDIS_DATA_PATH="${MYURLS_REDIS_DATA_PATH:-./data/redis}"
    PRE_UPGRADE_BACKUP='./backups/<Redis-7-成功冷备份目录>'
    export MYURLS_REDIS_DATA_PATH

    for tool in docker jq tar openssl awk grep find mv mktemp cmp rm; do command -v "$tool" >/dev/null; done
    test -d "$MYURLS_REDIS_DATA_PATH"
    data_source="$(cd "$MYURLS_REDIS_DATA_PATH" && pwd -P)"
    compose_source="$(docker compose config --format json |
      jq -er '.services["myurls-redis"].volumes[] | select(.target == "/data") | .source')"
    test "$data_source" = "$compose_source"
    MYURLS_REDIS_DATA_PATH="$data_source"
    export MYURLS_REDIS_DATA_PATH

    stamp="$(date +%Y%m%d-%H%M%S)"
    REDIS8_DATA_HOLD="${MYURLS_REDIS_DATA_PATH}.redis8-hold-${stamp}"
    REDIS7_STAGING="${MYURLS_REDIS_DATA_PATH}.redis7-staging-${stamp}"
    test -d "$PRE_UPGRADE_BACKUP"
    test -f "$PRE_UPGRADE_BACKUP/redis-manifest.env"
    test -f "$PRE_UPGRADE_BACKUP/redis-data.tar"
    test -f "$PRE_UPGRADE_BACKUP/SHA256SUMS"
    test ! -e "$REDIS8_DATA_HOLD"
    test ! -e "$REDIS7_STAGING"
    grep -qx 'redis_major=7' "$PRE_UPGRADE_BACKUP/redis-manifest.env"
    docker compose config --images | grep -Eq '^redis:7(\.|$)'

    (
      set -eu
      cd "$PRE_UPGRADE_BACKUP"
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
      if tar -tf redis-data.tar | grep -Eq '(^/|(^|/)\.\.(/|$))'; then false; fi
    )
    mkdir "$REDIS7_STAGING"
    tar -C "$REDIS7_STAGING" -xpf "$PRE_UPGRADE_BACKUP/redis-data.tar"
    find "$REDIS7_STAGING" -type f \
      \( -name '*.rdb' -o -name '*.aof' -o -name '*.manifest' \) | grep -q .

    docker compose stop --timeout 60 myurls
    app_id="$(docker compose ps --all --quiet myurls)"
    test -n "$app_id"
    test "$(docker inspect --format '{{.State.Running}}' "$app_id")" = false
    test "$(docker inspect --format '{{.State.ExitCode}}' "$app_id")" = 0
    test "$(docker inspect --format '{{.State.OOMKilled}}' "$app_id")" = false
    test -z "$(docker inspect --format '{{.State.Error}}' "$app_id")"
    docker compose stop --timeout 60 myurls-redis
    redis_id="$(docker compose ps --all --quiet myurls-redis)"
    test -n "$redis_id"
    test "$(docker inspect --format '{{.State.Running}}' "$redis_id")" = false
    test "$(docker inspect --format '{{.State.ExitCode}}' "$redis_id")" = 0
    test "$(docker inspect --format '{{.State.OOMKilled}}' "$redis_id")" = false
    test -z "$(docker inspect --format '{{.State.Error}}' "$redis_id")"

    moved_to_hold=false
    restore_hold() {
      if [ "$moved_to_hold" = true ] && [ ! -e "$MYURLS_REDIS_DATA_PATH" ] &&
         [ -d "$REDIS8_DATA_HOLD" ]; then
        mv "$REDIS8_DATA_HOLD" "$MYURLS_REDIS_DATA_PATH"
      fi
    }
    trap 'restore_hold' EXIT
    trap 'trap - EXIT HUP INT TERM; restore_hold; exit 1' HUP INT TERM
    mv "$MYURLS_REDIS_DATA_PATH" "$REDIS8_DATA_HOLD"
    moved_to_hold=true
    mv "$REDIS7_STAGING" "$MYURLS_REDIS_DATA_PATH"
    moved_to_hold=false
    trap - EXIT HUP INT TERM
  )
  switch_status=$?

  if [ "$switch_status" -ne 0 ]; then
    printf '%s\n' '回滚预检或原子切换失败：禁止启动任何服务。' >&2
    docker compose ps --all
    false
  else
    (
      set -eu
      docker compose up -d --wait --wait-timeout 120 myurls-redis
      docker compose logs --tail=200 myurls-redis
      docker compose exec -T myurls-redis sh -eu -c '
        if [ -n "$MYURLS_REDIS_PASSWORD" ]; then export REDISCLI_AUTH="$MYURLS_REDIS_PASSWORD"; fi
        version="$(redis-cli --raw INFO server | sed -n "s/^redis_version://p" | tr -d "\r")"
        case "$version" in 7.*) ;; *) false ;; esac
        redis-cli PING | grep -qx PONG
      '
      docker compose up -d --wait --wait-timeout 120 myurls
      curl --fail --silent --show-error http://localhost:8080/healthz
    )
    start_status=$?
    if [ "$start_status" -ne 0 ]; then
      docker compose ps --all
      docker compose logs --tail=200 myurls myurls-redis
      false
    fi
  fi
)
```

staging 与正式数据目录位于同一父目录，因此最后两次 `mv` 是同一文件系统内的目录切换，
不会把 Redis 7 文件合并进 Redis 8 目录。切换中途失败时 trap 会在正式路径缺失的条件下
尝试把 hold 移回；无论自动恢复是否成功，服务都保持停止。先检查三个路径；若正式路径
缺失且 hold 完整，可手工 `mv "$REDIS8_DATA_HOLD" "$MYURLS_REDIS_DATA_PATH"` 恢复原路径，
但仍须保持服务停止并重新调查。回滚验证完成前不要覆盖或删除 Redis 8 hold。

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
完整 Git commit SHA 标签，不发布 `latest`。

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
