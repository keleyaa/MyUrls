# MyURL 运维指南

本指南只覆盖 Rust 应用、当前 Redis 卷和部署者自有公网入口之间的责任边界。项目不安装或配置 Nginx、Caddy、Traefik、Cloudflare Tunnel、TLS、DNS 或主机防火墙。

## 部署前检查

```sh
corepack pnpm install --frozen-lockfile
cargo test -p myurl-server --all-features
corepack pnpm verify
```

生产环境必须满足：

- `NODE_ENV=production`。
- `PUBLIC_BASE_URL` 是 HTTPS origin，不含 path、query、fragment 或凭据。
- `IP_HASH_SECRET` 至少 32 个随机字节，不能使用示例值。
- `TURNSTILE_ENABLED=true`、`TURNSTILE_MODE=cloudflare`，并设置 site key、secret key 和预期 hostname。
- `TRUST_PROXY_CIDRS` 只填写实际控制并会清理转发头的代理网段，不使用信任全部的配置。
- `REDIS_URL` 指向 Compose 内部 `redis:6379`，若使用密码，凭据只放在受保护的环境文件中。

启动：

```sh
docker compose up -d --build --wait
docker compose ps
curl --fail --silent http://127.0.0.1:${APP_PORT:-3000}/health/live
curl --fail --silent http://127.0.0.1:${APP_PORT:-3000}/health/ready
```

`/health/live` 不访问 Redis；`/health/ready` 会在短超时内执行 Redis `PING`。应用容器以非 root 用户运行、根文件系统只读，Redis 只加入 Compose 内网。

## 公网入口

部署者的反向代理或隧道应只将 HTTPS 流量转发到 `127.0.0.1:${APP_PORT:-3000}`。项目不会自动信任任何转发头；配置 `TRUST_PROXY_CIDRS` 前，应验证入口会覆盖而不是追加客户端可伪造的 `X-Forwarded-For` 或 `Forwarded`。

公网入口还应负责 TLS、域名、证书续期、主机防火墙、异机备份复制和日志收集。项目应用层已经设置 CSP、禁止 frame、无 referrer、无存储缓存和 noindex 跳转头。

## 配置摘要

| 变量                      | 默认值                 | 说明                                                     |
| ------------------------- | ---------------------- | -------------------------------------------------------- |
| `NODE_ENV`                | 无                     | `development`、`test` 或 `production`；生产必填          |
| `LOG_LEVEL`               | `info`                 | Rust tracing 日志级别；性能压测或高吞吐部署可使用 `warn` |
| `APP_PORT`                | `3000`                 | 宿主机映射端口，应用容器端口固定为 3000                  |
| `PUBLIC_BASE_URL`         | 无                     | 生成短链使用的可信 HTTPS origin                          |
| `REDIS_URL`               | `redis://redis:6379/0` | Redis URL，支持 `redis` 和 `rediss`                      |
| `REDIS_PASSWORD`          | 空                     | Compose Redis 密码；应用会在启动时合并到 `REDIS_URL`     |
| `IP_HASH_SECRET`          | 无                     | HMAC 密钥，至少 32 字节                                  |
| `TRUST_PROXY_CIDRS`       | 空                     | 逗号分隔的可信代理 CIDR                                  |
| `TURNSTILE_ENABLED`       | `true`                 | 生产必须启用                                             |
| `TURNSTILE_SITE_KEY`      | 无                     | 仅作为挑战响应返回给浏览器                               |
| `TURNSTILE_SECRET_KEY`    | 无                     | 只在服务端验证 Turnstile                                 |
| `TURNSTILE_HOSTNAME`      | 无                     | 生产响应 hostname 校验值                                 |
| `CREATE_DIRECT_LIMIT_10M` | `5`                    | 10 分钟内免挑战创建数                                    |
| `CREATE_HARD_LIMIT_10M`   | `20`                   | 10 分钟硬上限                                            |
| `CREATE_HARD_LIMIT_1D`    | `100`                  | UTC 日硬上限                                             |
| `RESOLVE_LIMIT_10S`       | `600`                  | 单个 IP 在 10 秒内的短链解析上限                         |
| `RISK_CHALLENGE_SCORE`    | `3`                    | 触发挑战的风险分                                         |
| `RISK_BLOCK_SCORE`        | `8`                    | 触发 `429` 的风险分                                      |

配置在启动时一次解析并冻结。缺少密钥、生产使用 HTTP、限制关系错误、非法 CIDR 或测试模式进入生产都会导致非零退出。

## 日志与隐私

日志是 stdout 上的单行 JSON。允许字段包括 request ID、路由模板、状态、耗时、业务结果分类和依赖分类。禁止记录原始 IP、完整 HMAC、目标 URL、短码、别名、Location、请求体、响应体、Turnstile token、密钥和 Redis 凭据。短链解析按 IP 限流，超限返回 `429` 和 `Retry-After`；公网入口仍应配置更高层的 DDoS/WAF 防护。

Compose 使用 `json-file` 轮转，单文件 10 MB、最多 3 个文件。异机或对象存储复制由部署者负责。

## 备份

Redis 同时启用 AOF `appendfsync everysec` 和 RDB 快照。每日备份示例：

```sh
mkdir -p ops/backups
./ops/redis-backup.sh ops/backups
```

脚本使用 `redis-cli --rdb` 生成 RDB 和 SHA-256 sidecar，保留最近 7 个 RDB。RDB/AOF 包含短链目标 URL，应按敏感数据保护并在异机或对象存储侧加密。`ops/backups` 不应提交到 Git；生产环境应把结果复制到异机或对象存储。只保留在同一 VPS 不构成灾难恢复。

建议使用部署者自己的调度器每天运行一次，并保护备份目录和环境文件权限。备份过程不应把 Redis 密码放入命令行或日志。

## 恢复

恢复必须停止写入并恢复到新卷，确认抽样短链可解析后才能切换：

```sh
docker compose stop app redis
./ops/redis-restore.sh \
  /secure/backups/redis-20260826T020000Z.rdb \
   myurl-redis-restore-20260826 \
  launch \
  https://example.com/articles/launch
```

恢复脚本强制校验 sidecar，并要求传入一个短码和预期目标 URL；它会用临时 `appendonly no` Redis 加载 RDB，执行 `PING` 和短链抽样检查，再生成 AOF 基线。抽样不匹配、sidecar 缺失或卷已存在都会失败，失败时新建的临时卷会清理。脚本成功只代表新卷已经过校验，不会自动修改 Compose 或切换流量。确认成功后，用新卷启动 Redis，再次执行 `PING` 和短链抽样检查，最后才更新 Compose 的卷引用并启动应用。不要直接覆盖现有 `myurl-redis-data`，不要把旧 deployment 的 Redis 卷或 key 复制到当前数据集。候选发布的自动恢复演练由 `corepack pnpm backup:restore` 执行，并使用唯一临时卷。

目标是单 VPS 下 `RPO <= 24 小时`、`RTO <= 2 小时`；这不是高可用承诺。AOF、RDB 和异机备份提供可恢复性，不提供零停机或零数据丢失。

## 数据切换与回滚

Rust 发布使用 Compose 创建的 `myurl-redis-data` 新卷。旧 deployment 的 Redis 卷仅保留为回滚证据，不迁移、挂载或双读；因此旧短链在切换后不会由 Rust 服务解析。切换前先验证旧卷与备份可独立恢复，再停止旧应用，使用新卷启动 Rust deployment 并检查 `/health/live`、`/health/ready` 与创建/解析流程。

应用问题优先停止 Rust 写入、保留 `myurl-redis-data` 和备份，再按发布系统切回已验证的旧 deployment，并重新挂载它自己的旧 Redis 卷。不能只替换容器镜像来复用另一数据集；回滚后的新写入与 Rust 数据集同样不兼容。

生产切换、合并到 `master`、推送远端和流量切换都需要独立授权；本项目验证脚本不会执行这些操作。
