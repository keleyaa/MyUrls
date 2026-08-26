# MyURL v2

MyURL v2 是一个无账号、无统计的公开短链接工具。用户提交绝对 HTTP(S) URL 后，服务生成一个固定 90 天有效的短链接，并在浏览器允许时自动复制结果。

v2 使用 TypeScript、Fastify、Redis、Svelte 和 Vite。它是独立版本：不兼容、不迁移、也不复用 v1 的 Go API、Redis key 或数据卷。

## 快速开始

需要 Node.js 24、Corepack、Docker Compose v2、可用的 Chromium/WebKit 浏览器，以及本地 Trivy 0.74.0（完整 `verify` 的容器扫描门禁）。macOS 可执行 `brew install trivy`。

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm build
```

本地开发可以使用内存测试存储启动页面和服务：

```sh
NODE_ENV=test \
PUBLIC_BASE_URL=http://127.0.0.1:3000 \
IP_HASH_SECRET=local-development-secret-that-is-at-least-32-bytes \
TURNSTILE_ENABLED=false \
TEST_STORE=memory \
corepack pnpm dev:server
```

生产或接近生产的 Compose 栈：

```sh
cp .env.example .env
# 编辑 .env，至少设置 PUBLIC_BASE_URL、IP_HASH_SECRET 和 Turnstile 配置。
docker compose up -d --build --wait
```

Compose 只启动 `app` 和 `redis`。应用监听容器内 `0.0.0.0:3000`，宿主机默认只绑定 `127.0.0.1:${APP_PORT:-3000}`；Redis 不发布宿主机端口。公网入口、TLS、域名解析、防火墙和日志平台由部署者负责。

## HTTP 合同

创建短链接：

```sh
curl --fail-with-body http://127.0.0.1:3000/api/v1/links \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/docs","alias":"docs"}'
```

成功返回 `201`，响应包含 `code`、`shortUrl` 和 UTC `expiresAt`。自动短码为 8 位大小写敏感 Base62；别名规范化为 ASCII 小写，并与自动短码共享 Redis `NX` 命名空间。

短链 `GET` 和 `HEAD` 返回 `302`，解析失败返回 `404`，Redis 故障返回 `503`。创建接口的稳定错误码包括 `invalid_request`、`challenge_required`、`challenge_invalid`、`alias_unavailable`、`url_not_allowed`、`alias_invalid`、`rate_limited`、`dependency_unavailable` 和 `code_generation_exhausted`。

## 工程结构

```text
apps/web/                 Svelte 页面、状态机和本地资源
apps/server/              Fastify、ShortLinkService 和适配器
packages/contracts/       TypeBox JSON Schema 与共享类型
tests/e2e/                Chromium、移动 Chromium、WebKit 流程
ops/                      Compose 验证、备份、恢复和安全扫描
```

`ShortLinkService` 是对外业务接口的深模块：路由只处理 schema、可信来源上下文和 HTTP 映射；URL 策略、别名、风险、Redis 原子写入和 TTL 都隐藏在服务接口之后。Redis Adapter 与 Turnstile Adapter 是可替换的真实外部 seam。

## 验证

单项检查：

```sh
corepack pnpm format
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm test:api
corepack pnpm test:integration
corepack pnpm build
corepack pnpm test:e2e
```

候选版本统一门禁：

```sh
corepack pnpm verify
```

`verify` 会执行格式、ESLint、严格 TypeScript、覆盖率、真实 Redis、API 合同、生产构建、浏览器流程、Compose 重启持久化、备份恢复、依赖审计、容器高危漏洞扫描和运行时外部资源检查。任一步失败都会返回非零状态。

## 隐私与安全边界

- 原始 IP 不写入 Redis 或日志；限流 key 使用 HMAC-SHA-256 指纹。
- API 默认不信任 `X-Forwarded-For` 和 `Forwarded`，只有 `TRUST_PROXY_CIDRS` 明确匹配时才解析。
- 服务端不连接、解析 DNS、抓取或预览目标 URL，因此 v2 不做目标可用性检查或 DNS 重绑定检测。
- Turnstile 只有风险达到阈值后才加载；token 不记录、不缓存、不复用。
- 应用资源、字体和图标随构建产物打包；除按需 Turnstile 外不使用运行时第三方资源。
- v2 Redis 使用独立命名卷 `myurl-v2-redis-data`，不会读取 v1 数据目录。

## 运维

部署和恢复步骤见 [v2 运维指南](docs/operations.md)。备份默认不写入 Git；候选发布至少需要在干净环境执行一次恢复演练。

项目链接：[GitHub](https://github.com/keleyaa/MyUrls)。订阅转换入口：[sub.ml1.one](https://sub.ml1.one)。许可证：MIT。
