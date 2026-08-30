# MyURL Rust 后端重构实现计划

> **归档状态：** 此计划已完成。它保留逐任务实施细节，供后续维护或审计使用；不应被当作待执行清单。

**目标：** 用 Rust/Axum 替换 TypeScript/Fastify 后端，保留 Svelte 前端和 Node/shell 运维工具，使稳定的创建入口变为 `/api/links`，并完成 Redis、HTTP、安全和部署验证。

**架构：** Rust 单体二进制负责配置、领域规则、IP 指纹、Redis、Turnstile、Axum HTTP 和 Svelte 静态文件托管。领域层只依赖 `LinkStore` 与 `ChallengeVerifier` 小接口，生产使用 Redis/Cloudflare adapter，测试使用内存/fake adapter。切换使用新的 `myurl-redis-data` 卷，不迁移旧 v2 数据。

**技术栈：** Rust 1.88、Axum 0.8、Tokio 1、Serde、Redis 0.29、Reqwest 0.12、URL/IPNet、HMAC/SHA-256、Time、Tracing、Tower HTTP、Cargo；Svelte/Vite、TypeScript contracts、Playwright 和现有 Node/shell 运维编排继续保留。

---

> 状态：已完成（已合并到 `master`）
>
> 依据：[Rust 后端重构设计规格](../specs/2026-08-29-rust-backend-redesign-design.md)
>
> 实施结果：Rust 1.88 生产镜像替代 TypeScript/Fastify 服务端；已移除的 legacy TypeScript server、其 Vitest 配置和 workspace 引用已删除；公开创建入口为 `/api/links`；生产 Redis 使用新的 `myurl-redis-data` 卷。`corepack pnpm verify` 已在合并后的 `master` 上通过，覆盖 Rust、前端、浏览器、Docker、Compose、性能、备份恢复与安全检查。

## 1. 实施约束

- 保留匿名创建、短码解析、固定 90 天 TTL、Redis 原子占位、IP 指纹限流、Turnstile、SSRF 防护、隐私日志和健康检查。
- Rust 运行时只负责 HTTP 服务、领域规则、Redis、Turnstile 和 Svelte 静态文件托管。
- `apps/web`、`packages/contracts`、Playwright 测试和现有页面状态机继续使用 TypeScript/Svelte；不引入 Rust/WASM 或 SSR。
- `ops/*.mjs`、`ops/*.sh` 不重写为 Rust。只修改其中引用 旧的版本化创建路径 的验证脚本；Redis 备份/恢复脚本继续调用官方 `redis-cli`。
- 每完成一个逻辑阶段都执行对应测试，并创建一个 Conventional Commit；不在未通过阶段测试时删除旧 TypeScript 服务。
- Rust 生产构建使用 Cargo.lock 和官方 Rust builder；最终镜像只包含一个 Rust 二进制、前端静态文件、CA 证书和健康检查所需的 HTTP 客户端。

每个任务遵循同一执行节奏：

- [x] 已迁移或新增各任务列出的测试，覆盖成功、失败和边界行为。
- [x] 已运行最小测试和系统验证，并以失败结果驱动修复。
- [x] 已完成满足任务接口与安全约束的最小实现。
- [x] 已运行格式、类型、浏览器、Docker 和集成检查。
- [x] 已在验证通过后按 Conventional Commit 拆分提交。

## 2. 目标目录

完成后新增以下 Rust 工作区结构：

```text
Cargo.toml
Cargo.lock
crates/
  myurl-server/
    Cargo.toml
    src/
      main.rs
      lib.rs
      config.rs
      error.rs
      ip.rs
      ports.rs
      service.rs
      redis.rs
      turnstile.rs
      http.rs
      domain/
        mod.rs
        alias.rs
        url_policy.rs
        short_code.rs
        risk.rs
        time.rs
      testing.rs
    tests/
      http.rs
      redis.rs
      service.rs
```

最终删除：

```text
legacy server 的 package manifest
legacy server 的 TypeScript 配置
legacy server 的 TypeScript 源码
```

已移除的 legacy TypeScript server 删除前必须完成 Rust 单元、HTTP 集成和 Redis 集成测试的迁移，并由 Compose/E2E 验证覆盖运行时路径。

## 3. 阶段一：建立 Cargo 工作区和可编译入口

### 任务 1：添加 Cargo workspace 和 crate manifest

修改或创建：

- `Cargo.toml`
- `crates/myurl-server/Cargo.toml`

根工作区声明 `resolver = "2"` 和 member `crates/myurl-server`。服务 crate 使用 Rust 2024 edition，并声明库和二进制入口。依赖使用以下职责划分：

- `axum 0.8`、`tokio 1`：HTTP 和异步运行时。
- `serde 1`、`serde_json 1`：请求、响应和配置数据。
- `async-trait 0.1`：`LinkStore`、`ChallengeVerifier` 的对象安全异步接口。
- `redis 0.29` 的 Tokio 支持：Redis 命令、脚本和连接池外的单连接封装。
- `reqwest 0.12` 的 JSON 和 rustls 支持：Turnstile HTTPS 调用。
- `url 2`、`ipnet 2`：URL 解析和 CIDR 判断。
- `hmac 0.12`、`sha2 0.10`、`rand 0.9`：IP 指纹和安全短码。
- `time 0.3`：UTC 时间和 RFC 3339 格式化。
- `thiserror 2`：显式领域、存储和挑战错误。
- `tracing 0.1`、`tracing-subscriber 0.3`：结构化日志和 `LOG_LEVEL` 过滤。
- `tower-http 0.6`：静态文件和请求层能力。
- `uuid 1`：没有合规调用方 request ID 时生成新 ID。

开发依赖增加 `tower 0.5`，用于 HTTP 集成测试的请求注入；增加 `wiremock 0.6`，用于 Turnstile HTTP adapter 测试。定义 `test-support` feature 供需要时编译内存 adapter；生产二进制不启用该 feature。

实施内容：

根目录 `Cargo.toml`：

```toml
[workspace]
resolver = "2"
members = ["crates/myurl-server"]
```

`crates/myurl-server/Cargo.toml`：

```toml
[package]
name = "myurl-server"
version = "2.0.2"
edition = "2024"
rust-version = "1.88"

[features]
test-support = []

[[bin]]
name = "myurl-server"
path = "src/main.rs"
```

执行：

```sh
cargo metadata --no-deps
cargo fmt --check
```

首次格式检查可以因尚未有源码而不通过；添加入口后再次执行，并提交：`build: 建立 Rust 服务工作区`。

### 任务 2：建立库入口和最小 main

创建：

- `crates/myurl-server/src/lib.rs`
- `crates/myurl-server/src/main.rs`

`lib.rs` 声明后续模块，并导出 `build_app`、`AppConfig`、`AppError` 和测试所需的 `testing` 模块。`main.rs` 只负责读取环境、初始化 tracing、构造 store/verifier、调用 `run`；不得在 main 中实现领域规则。

先让 `main` 返回显式 `Result<(), Box<dyn Error>>`，监听地址暂时由 `config.port` 生成，HTTP router 可以先返回 live 响应。此步骤只验证 Cargo 编译链，不实现业务。

执行：

```sh
cargo check -p myurl-server
cargo test -p myurl-server
```

## 4. 阶段二：迁移配置、错误和纯领域规则

### 任务 3：迁移启动配置解析

创建：

- `crates/myurl-server/src/config.rs`
- `crates/myurl-server/src/config.rs` 内的 `#[cfg(test)]` 测试

实现 `AppConfig::from_env`，保留现有环境变量名和校验关系：

- `NODE_ENV` 只接受 `development`、`test`、`production`。
- `APP_PORT` 为 `1..=65535` 的十进制整数。
- `PUBLIC_BASE_URL` 只接受无 path/query/fragment/凭据的 HTTP(S) origin，生产必须 HTTPS。
- `REDIS_URL` 只接受 `redis`/`rediss`，数据库范围 `0..=15`；`REDIS_PASSWORD` 与 URL 密码合并时校验一致性。
- `IP_HASH_SECRET` 至少 32 个 UTF-8 字节，生产拒绝示例值。
- 保留所有创建、解析、风险、Redis、Turnstile、请求和关闭超时配置及默认值。
- 保留 `TRUST_PROXY_CIDRS` 的 CIDR 解析和生产环境拒绝 `/0`。
- 保留 Turnstile 生产配置、测试模式和 `TEST_FORCE_CHALLENGE`/`TEST_STORE` 的环境限制。
- 启动时拒绝 `hard10m <= direct10m`、`hard1d <= hard10m` 或 `blockScore <= challengeScore`。

固定常量：`LINK_TTL_SECONDS = 7_776_000`、`MAX_URL_BYTES = 4096`、`MAX_BODY_BYTES = 16 * 1024`、自动短码长度 10、最多 5 次自动短码占位。

测试迁移 `legacy server 源码目录/config.ts` 的成功默认值和每个失败分支，断言失败只返回配置错误，不记录 secret 或 Redis 密码。

执行：

```sh
cargo test -p myurl-server config
```

### 任务 4：建立显式错误类型和稳定错误码

创建：

- `crates/myurl-server/src/error.rs`
- `crates/myurl-server/src/ports.rs`

在 `error.rs` 中定义：

- `ErrorCode`：`invalid_request`、`challenge_required`、`challenge_invalid`、`alias_unavailable`、`url_not_allowed`、`alias_invalid`、`rate_limited`、`dependency_unavailable`、`code_generation_exhausted`。
- 领域错误：URL、alias、挑战、限流、冲突、生成耗尽。
- `StoreError` 和 `ChallengeError`，内部可携带 source，但不得直接序列化给客户端。
- `AppError`，区分 HTTP 校验错误、领域错误、存储错误和挑战依赖错误。

在 `ports.rs` 中定义：

```rust
#[async_trait]
pub trait LinkStore: Send + Sync {
    async fn claim(&self, code: &str, target_url: &str, ttl: Duration) -> Result<bool, StoreError>;
    async fn lookup(&self, code: &str) -> Result<Option<String>, StoreError>;
    async fn increment_resolve_counter(&self, fingerprint: &str) -> Result<u64, StoreError>;
    async fn increment_create_counters(&self, fingerprint: &str, utc_date: &str) -> Result<CreateCounts, StoreError>;
    async fn risk_score(&self, fingerprint: &str) -> Result<u64, StoreError>;
    async fn add_risk_score(&self, fingerprint: &str, points: u64) -> Result<u64, StoreError>;
    async fn ping(&self) -> Result<(), StoreError>;
    async fn close(&self) -> Result<(), StoreError>;
}

#[async_trait]
pub trait ChallengeVerifier: Send + Sync {
    async fn verify(&self, token: &str) -> Result<bool, ChallengeError>;
}
```

`CreateCounts`、`CreateResult`、`Challenge` 等跨模块值对象也放在 `ports.rs` 或 `domain/mod.rs`，不得让 HTTP handler 依赖 Redis client 类型。

迁移 `legacy server 源码目录/errors.ts` 的状态映射和 `legacy server 源码目录/ports.ts` 的接口语义。为每个 `ErrorCode` 添加状态码映射测试，确认未知 adapter 错误统一转为 `dependency_unavailable`。

执行：

```sh
cargo test -p myurl-server error
cargo check -p myurl-server
```

### 任务 5：迁移 URL 安全策略

创建：

- `crates/myurl-server/src/domain/url_policy.rs`
- `crates/myurl-server/src/domain/mod.rs`

实现 `normalize_target_url(input: &str) -> Result<String, DomainError>`：

1. 先按 UTF-8 字节数限制 4096。
2. 拒绝空白、控制字符和格式字符；URL 编码后的 `%0A` 保持可接受，字面控制字符必须拒绝。
3. 使用 `url::Url` 解析绝对 URL，仅接受 `http` 和 `https`。
4. 拒绝空 hostname、用户名和密码。
5. 对 hostname 去除末尾点后比较保留后缀：`localhost`、`local`、`internal`、`home.arpa`；精确命中或子域命中都拒绝。
6. 如果 hostname 是 IP literal，使用 `ipnet` 拒绝 unspecified、broadcast、multicast、link-local、loopback、private、unique-local、carrier-grade NAT、benchmark、reserved 和 documentation 范围。
7. 返回 URL parser 的规范化序列化结果，不发起 DNS、HTTP 或 TLS 请求。

把 `legacy server 源码目录/domain/url-policy.unit.test.ts` 的每个用例迁移为 Rust 参数化测试，特别保留凭据、`localhost`、内部后缀、RFC 1918、链路本地、回环、文档 IP、IPv6 unique-local 和 Unicode 字节长度测试。

执行：

```sh
cargo test -p myurl-server url_policy
```

### 任务 6：迁移别名、短码、风险和时间策略

创建：

- `crates/myurl-server/src/domain/alias.rs`
- `crates/myurl-server/src/domain/short_code.rs`
- `crates/myurl-server/src/domain/risk.rs`
- `crates/myurl-server/src/domain/time.rs`

实现以下纯函数：

- `normalize_alias`：裁剪、ASCII 小写化，接受 4–32 位 `[a-z0-9_-]`；大写输入归一化，非 ASCII、短/长、点、空格、斜线和百分号拒绝。
- `is_reserved_code`：大小写不敏感拒绝 `api`、`health`、静态资源路径及现有保留名称，包括 `favicon.ico`。
- `generate_short_code`：从 OS 安全随机源生成 10 位 Base62，使用无偏抽样；`is_valid_code` 接受解析路径允许的 4–32 位字符形状。
- `evaluate_risk`：先判断硬限流，再判断 challenge，再允许；challenge 禁用时只有硬限流仍生效，`forceChallenge` 只在测试配置中启用。
- `expiry_at`：基于注入的当前 UTC 时间加 90 天；`utc_date` 生成 Redis 日计数 key 使用的 `YYYY-MM-DD`。

随机短码测试注入确定性随机源，覆盖全零、合法字符、非法形状和生成尝试上限；不得用固定字符串作为生产随机源。

执行：

```sh
cargo test -p myurl-server domain
```

提交：`feat: 添加 Rust 短链领域规则`。

### 任务 7：迁移 IP 解析、代理链和 HMAC 指纹

创建：

- `crates/myurl-server/src/ip.rs`
- `crates/myurl-server/src/ip.rs` 内的单元测试

使用 `std::net::{IpAddr, SocketAddr}` 和 `ipnet::IpNet` 实现：

- 去除 IPv6 方括号并 canonicalize IP。
- 远端地址不在可信 proxy CIDR 时，忽略所有 `X-Forwarded-For`/`Forwarded`。
- 远端地址可信时，按右到左消费转发链；遇到非法、`unknown` 或 `_hidden` 停止信任。
- 兼容现有 `X-Forwarded-For` 优先、`Forwarded for=` 回退规则。
- 使用 HMAC-SHA-256 和 `IP_HASH_SECRET` 输出十六进制 fingerprint；原始 IP 不进入日志或请求响应。

把现有 IP 单元测试迁移，并加入 IPv4-mapped IPv6、带方括号地址、伪造转发头和非法 CIDR 测试。

执行：

```sh
cargo test -p myurl-server ip
```

## 5. 阶段三：内存 adapter 和短链服务

### 任务 8：建立内存 LinkStore 和测试 Turnstile

创建：

- `crates/myurl-server/src/testing.rs`

使用 `Arc<Mutex<...>>` 保存链接、计数器、风险分和故障开关，实现 `LinkStore`：

- `claim` 必须模拟 NX 语义，已有 code 不覆盖。
- 计数器首次写入记录 TTL 的逻辑值，测试可检查 600、172800、10 和 600 秒。
- `lookup` 返回过期值前先清理过期项。
- `ping`、`close`、每个 command 都支持可控失败，以覆盖 503 映射。

实现 `FakeTurnstile`：默认只接受 `test-token`，可配置 invalid 或 unavailable，并记录调用次数。实现可注入的时钟和短码生成器，供服务测试固定时间和冲突序列。

测试模块不从环境读取密钥、不打印目标 URL，并且不在 `main` 的生产构造路径中被选用；只有 `NODE_ENV=test` 且 `TEST_STORE=memory` 才允许使用。

执行：

```sh
cargo test -p myurl-server --all-features testing
```

### 任务 9：实现 ShortLinkService 创建和解析编排

创建：

- `crates/myurl-server/src/service.rs`
- `crates/myurl-server/tests/service.rs`

定义 `ShortLinkService`，依赖 `Arc<dyn LinkStore>`、`Arc<dyn ChallengeVerifier>`、`AppConfig`、注入时钟和短码生成器。创建流程严格按以下顺序：

```text
递增创建计数器
-> 读取风险分
-> 硬限流 / challenge / allow 决策
-> challenge token 校验；失败增加 3 分
-> URL 规范化
-> alias 规范化和保留名称校验
-> alias 或自动短码 SET NX EX
-> alias 冲突增加 1 分；自动短码冲突重试最多 5 次
-> 计算 expiresAt 和可信 public origin
-> 返回 code、shortUrl、expiresAt
```

解析流程：

```text
客户端地址 -> HMAC fingerprint
-> 原子递增 10 秒解析计数器
-> 超限返回 RateLimited
-> 短码格式检查
-> lookup
-> None 返回 NotFound，命中返回目标 URL
```

服务层不处理 HTTP status、不读取请求头、不写日志、不访问目标地址。所有 store/Turnstile 错误显式转为依赖错误；风险记录失败不能被忽略。

迁移 `legacy server 源码目录/service.unit.test.ts` 的全部用例，至少覆盖：

- 自动 code 和固定过期时间。
- alias 标准化、保留 alias、alias 冲突。
- URL 失败和 alias 失败的风险分。
- 直接阈值后的 challenge、有效/无效/不可用 token。
- 硬限流和已有风险分。
- 自动 code collision 重试和耗尽，不覆盖旧值。
- resolve 未命中、命中、格式错误和 store 故障。

执行：

```sh
cargo test -p myurl-server --all-features --test service
```

提交：`feat: 实现 Rust 短链服务编排`。

## 6. 阶段四：Redis 和 Turnstile adapter

### 任务 10：实现 RedisLinkStore

创建：

- `crates/myurl-server/src/redis.rs`
- `crates/myurl-server/tests/redis.rs`
- 更新 `ops/run-redis-integration.mjs`

实现单连接 `RedisLinkStore::connect`，连接时设置不可重连策略和 `REDIS_TIMEOUT_MS`；每个 Redis 命令用同一超时包装器，超时、连接关闭、返回类型错误均转为 `StoreError::Unavailable`。

保持稳定业务 key：

```text
myurl:link:{code}
myurl:rate:create:10m:{fingerprint}
myurl:rate:create:1d:{utc_date}:{fingerprint}
myurl:rate:resolve:10s:{fingerprint}
myurl:risk:create:10m:{fingerprint}
```

实现并测试三段 Lua：

```lua
local short_count = redis.call('INCR', KEYS[1])
if short_count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
local daily_count = redis.call('INCR', KEYS[2])
if daily_count == 1 then redis.call('EXPIRE', KEYS[2], ARGV[2]) end
return { short_count, daily_count }
```

```lua
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return count
```

```lua
local existed = redis.call('EXISTS', KEYS[1])
local score = redis.call('INCRBY', KEYS[1], ARGV[1])
if existed == 0 then redis.call('EXPIRE', KEYS[1], ARGV[2]) end
return score
```

TTL 必须保持：链接 7,776,000 秒，创建 10 分钟 600 秒，日计数 172800 秒，解析 10 秒，风险 600 秒。`close` 在超时后销毁连接，不阻塞 shutdown。

Redis 集成测试迁移 `legacy server 源码目录/redis.integration.test.ts`，覆盖 NX、TTL、两个计数器原子性、风险累加、20 个并发 alias 只有一个 winner、过期、关闭连接失败和重启后数据可读。

更新 `ops/run-redis-integration.mjs`：继续由 Node 负责临时 Compose 生命周期，但将 `vitest` 命令替换为：

```text
cargo test -p myurl-server --all-features --test redis -- --ignored
```

脚本仍使用 `ops/docker-compose.verify.yaml` 的隔离端口和 `finally` 清理卷。

执行：

```sh
node ops/run-redis-integration.mjs
```

### 任务 11：实现 Cloudflare Turnstile adapter

创建：

- `crates/myurl-server/src/turnstile.rs`
- `crates/myurl-server/src/turnstile.rs` 内的单元测试

使用 `reqwest::Client` 和单次请求超时调用：

```text
POST https://challenges.cloudflare.com/turnstile/v0/siteverify
Content-Type: application/x-www-form-urlencoded
secret=TURNSTILE_SECRET_KEY&response={token}
```

响应反序列化为受限结构，只接受 `success === true`；生产环境额外要求 hostname 等于 `TURNSTILE_HOSTNAME`、action 等于 `create_link`。HTTP 非 2xx、超时、JSON 形状错误和 TLS 错误返回 `ChallengeError::Unavailable`；provider 明确返回失败时返回 `Ok(false)`。不记录 token、secret、响应体。

测试使用 `wiremock` 作为 dev dependency，覆盖成功、provider invalid、hostname/action 不匹配、非 2xx、超时和 malformed JSON。测试实现仍保留给 `TURNSTILE_MODE=test`。

执行：

```sh
cargo test -p myurl-server turnstile
```

## 7. 阶段五：Axum HTTP adapter 和进程生命周期

### 任务 12：定义 HTTP DTO 和 Problem Details

修改：

- `crates/myurl-server/src/http.rs`
- `crates/myurl-server/src/error.rs`

使用 Serde 定义：

- `CreateLinkRequest`：`url`、可选 `alias`、可选 `challengeToken`，`deny_unknown_fields`。
- `CreateLinkResponse`：`code`、`shortUrl`、`expiresAt`，使用 camelCase。
- `ProblemDetails`：`type`、`title`、`status`、`code`、`requestId`、可选 `retryAfterSeconds`、可选 `challenge`。

`ProblemDetails` 响应的 Content-Type 固定为 `application/problem+json`。`type` 使用可信 public origin 加 `/problems/{code}`，不得把错误 source、目标 URL、请求体或 token 放入 title/detail。request ID 规则固定为首字符字母或数字、后续只允许字母数字和 `._:-`、长度 1–80；不合规的 `X-Request-ID` 忽略并生成 UUID。

### 任务 13：实现路由、校验和安全响应头

实现：

- `POST /api/links`：body limit 16 KiB、JSON Content-Type、Serde 字段校验、Origin 校验、客户端地址提取，调用 service，成功返回 201 JSON。
- `GET /:code` 和 `HEAD /:code`：调用 resolve，命中返回 302 和 `Location`；HEAD body 为空。
- `GET /health/live`：不触碰 store，返回 200。
- `GET /health/ready`：在 `REDIS_TIMEOUT_MS` 内调用 `ping`，成功 200，失败 503。
- 未知 `/api/*`：Problem Details 的 404/invalid_request；未知浏览器路径：静态资源或安全 404 HTML。

所有响应设置：

```text
Cache-Control: no-store
Content-Security-Policy
Permissions-Policy
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
X-Robots-Tag: noindex, nofollow
```

限流响应额外设置 `Retry-After`，challenge 响应包含 public site key。错误映射必须使用显式 match，未知错误只能返回 `dependency_unavailable` 或 HTTP 层 `invalid_request`。

HTTP handler 不记录任何 code、alias、target URL、Location、IP、fingerprint、request body、response body 或 token；tracing 只记录 request ID、路由模板、状态、耗时、业务结果分类和依赖分类。

### 任务 14：实现静态托管、构造函数和优雅关闭

修改：

- `crates/myurl-server/src/http.rs`
- `crates/myurl-server/src/lib.rs`
- `crates/myurl-server/src/main.rs`

用 `tower_http::services::ServeDir` 从 `WEB_ROOT` 托管 Svelte `dist`，让明确 API/health/短码路由优先于静态 fallback。生产缺少静态文件时启动失败或安全返回 404，不能把内部路径返回浏览器。

`main.rs` 的运行顺序：

1. 解析并冻结 `AppConfig`。
2. 根据 `TEST_STORE=memory` 只在 test 环境构造内存 store，否则连接 Redis。
3. 根据 Turnstile mode 构造 Cloudflare 或 test verifier。
4. 构造 app 并绑定 `0.0.0.0:APP_PORT`。
5. 监听 SIGINT/SIGTERM。
6. 在 `SHUTDOWN_TIMEOUT_MS` 内停止接受新请求并调用 `store.close()`。
7. 超时记录错误并以非零状态退出。

执行：

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check -p myurl-server
```

### 任务 15：迁移 HTTP 集成测试

创建：

- `crates/myurl-server/tests/http.rs`

使用内存 adapter 和 `tower::ServiceExt::oneshot`，迁移 `legacy server 源码目录/http.api.test.ts` 的所有断言：

- `/api/links` 成功返回 201、使用配置 origin、忽略 Host。
- 非 JSON、malformed JSON、未知字段、超大 body、跨 origin 返回 400。
- URL/alias 策略错误返回 422。
- alias 冲突返回 409 且不泄露提交的 URL/token。
- challenge required/invalid/unavailable 的状态、challenge payload 和调用次数。
- 429 的 `Retry-After`。
- live/ready、redirect、HEAD、404、429、503。
- 浏览器安全响应头、request ID、Problem Details Content-Type 和字段结构。
- 静态文件存在和缺失路径。

每个测试使用独立 app/store，测试后关闭 store；不得依赖真实 Redis 或环境中的公网配置。

执行：

```sh
cargo test -p myurl-server --all-features --test http
```

提交：`feat: 暴露 Rust HTTP 短链接口`。

## 8. 阶段六：前端 contracts/API 适配

### 任务 16：更新 TypeScript contracts

修改：

- `packages/contracts/src/index.ts`
- `packages/contracts/tsconfig.json` 保持现有构建方式

将错误类型从旧的嵌套 `error` 结构扩展为与 Rust 的 Problem Details 对齐：

```ts
export type ErrorCode =
  | 'invalid_request'
  | 'challenge_required'
  | 'challenge_invalid'
  | 'alias_unavailable'
  | 'url_not_allowed'
  | 'alias_invalid'
  | 'rate_limited'
  | 'dependency_unavailable'
  | 'code_generation_exhausted';

export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  code: ErrorCode;
  requestId: string;
  retryAfterSeconds?: number;
  challenge?: Challenge;
}
```

保留 `CreateLinkInput` 的字段语义和 `CreateLinkResponse` 的 camelCase；用 TypeBox schema 继续约束前端可消费的 JSON，`challengeToken` 继续映射为服务端字段。

执行：

```sh
corepack pnpm --filter @myurl/contracts build
```

### 任务 17：修改 web API client，保持页面状态机

修改：

- `apps/web/src/lib/api.ts`

将创建请求路径从 旧的版本化创建路径 改为 `/api/links`。`ApiError` 改为从 Problem Details 读取 `code`、`challenge`、`retryAfterSeconds`；兼容失败响应非 JSON 的 fallback，但不把服务端文本直接展示给用户。保留 `checkReady` 的 `/health/ready` 路径、复制失败回退、挑战重试和现有页面状态转换。

为 `api.ts` 增加最小的 runtime guard：确认 `type`、`status`、`code`、`requestId` 类型正确后才构造 `ApiError`，无效响应统一作为 client-side `dependency_unavailable`。

执行：

```sh
corepack pnpm --filter @myurl/web check
corepack pnpm --filter @myurl/web build
```

## 9. 阶段七：E2E、Compose、Docker 和 CI

### 任务 18：更新 E2E 和 Node 编排脚本中的 API 路径

修改：

- `tests/e2e/app.spec.ts`
- `ops/compose-smoke.mjs`
- `ops/performance.mjs`
- `ops/run-redis-integration.mjs`

只把创建请求从 旧的版本化创建路径 改为 `/api/links`；不改变浏览器交互、Turnstile mock、复制 fallback、viewport 检查或性能并发参数。备份/恢复脚本中的 `myurl:link:{code}` key 不变。

更新 E2E 启动命令：

```text
cargo build --release
WEB_ROOT=apps/web/dist cargo run --release -p myurl-server
```

将 Playwright `webServer.command` 固定为先构建 contracts/web，再启动 Rust server；不要再调用 旧的 TypeScript server 开发命令。

执行：

```sh
corepack pnpm test:e2e
```

### 任务 19：重写 Dockerfile 的 builder/runtime 阶段

修改：

- `Dockerfile`

采用四段构建：

1. `rust-builder`：官方 Rust 1.88 bookworm builder，复制 Cargo manifest 和 lockfile，先 `cargo fetch`，再复制 `crates`，执行 `cargo build --release --locked`。
2. `web-dependencies`：沿用当前 Node 24 digest 和 Corepack，复制 pnpm workspace manifest 与 lockfile，执行 frozen install。
3. `web-builder`：复制前端和 contracts，执行 `pnpm build`，产出 `apps/web/dist`。
4. `runtime`：使用 Debian slim，安装 ca-certificates 和 curl，创建 UID 10001 的非 root 用户，复制 Rust 二进制到 `/usr/local/bin/myurl-server`，复制静态文件到 `/app/web`，设置 `WEB_ROOT=/app/web`，只暴露 3000。

运行时必须：

- `USER 10001:10001`。
- `read_only` 兼容，不依赖写入工作目录。
- 只保留 ca-certificates、curl、Rust 二进制和 web dist。
- `CMD ["/usr/local/bin/myurl-server"]`。
- healthcheck 使用 `curl --fail --silent http://127.0.0.1:3000/health/live`。

继续使用已有 Node builder 的 digest pinning，并为 Rust builder 使用官方镜像的固定 digest；不要在最终层保留 Node、npm、pnpm、Cargo 或源码。

执行：

```sh
docker compose build
```

### 任务 20：更新 Compose、环境文档和切换卷

修改：

- `docker-compose.yaml`
- `.env.example`
- `docs/operations.md`
- `README.md`

Compose 变化：

- app 使用 Rust runtime，不再设置 `user: node`。
- healthcheck 使用 Rust runtime 内的 curl。
- `WEB_ROOT=/app/web`。
- 环境变量名和默认值保持现有名称。
- Redis 服务继续不发布宿主机端口。
- 将持久卷从 `myurl-v2-redis-data` 改为 `myurl-redis-data`，作为一次性新数据集切换；不要在应用代码中加入 `v2`/`v3` key 前缀。
- Compose 仍等待 Redis healthy 后启动 app。

README 和运维文档更新：

- curl 创建示例改为 `/api/links`。
- 本地后端启动命令改为 Rust/Cargo，保留前端 Node/Vite 说明。
- 开发前置条件增加 Rust/Cargo；Node 仍用于 Svelte 构建和现有运维脚本。
- 明确新 Rust 服务不读取旧 Redis 数据集；切换前先备份，切换后验证新创建/跳转/HEAD/404/health。
- 明确 `ops/redis-backup.sh`、`ops/redis-restore.sh` 继续使用 shell/redis-cli，不属于第一阶段 Rust 迁移。

执行：

```sh
corepack pnpm format:write
corepack pnpm compose:build
```

### 任务 21：更新根脚本和 CI

修改：

- `package.json`
- `vitest.config.ts`
- 删除 `vitest.api.config.ts`
- 删除 `vitest.integration.config.ts`
- `playwright.config.ts`
- `.github/workflows/ci.yml`

根脚本调整为：

- `typecheck`：contracts + web，不再执行 已移除的 legacy TypeScript server TypeScript typecheck。
- `build`：contracts/web 构建 + `cargo build --release --locked`。
- `test:unit`：`cargo test --workspace --all-features` 加上仍存在的 TypeScript package unit tests。
- `test:api`：`cargo test -p myurl-server --all-features --test http`。
- `test:integration`：保留 Node 编排脚本，脚本内部执行 Rust Redis integration。
- `dev:server`：`cargo run -p myurl-server`。
- `verify`：保留前端、浏览器、Compose、性能、备份恢复、安全检查顺序，同时加入 `cargo fmt --check`、`cargo clippy ... -D warnings`、`cargo test --workspace --all-features`。

`vitest.config.ts` 删除 `legacy server 源码目录/domain` 和 `legacy server 源码目录/service.ts` coverage include；在没有 TypeScript unit 测试时保留配置给 contracts/web，避免让已删除 server 路径继续成为 coverage 门槛。

CI：

- 在 checkout 后安装 Rust stable，并安装 rustfmt/clippy 组件；按照仓库现有 action SHA pinning 规则固定 action。
- Rust cache key 包含 `Cargo.lock`。
- 运行 `cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace --all-features`。
- Node 仍安装，用于 Svelte build、Playwright 和未迁移的 ops 编排。
- CI job 名称从 TypeScript 后端改为 Rust backend、Svelte、Redis、Compose、browser 和 security verification。

执行：

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm test:api
```

## 10. 阶段八：删除旧服务并执行切换验证

### 任务 22：删除 TypeScript server 和旧测试入口

在 Rust HTTP/API/Redis/E2E/Compose 验证全部通过后，删除：

- `legacy server 的 package manifest`
- `legacy server 的 TypeScript 配置`
- `legacy server 源码目录/`
- `vitest.api.config.ts`
- `vitest.integration.config.ts`

更新：

- `pnpm-workspace.yaml` 不需要加入 Rust crate；它继续只管理 web/contracts Node workspace。
- `pnpm-lock.yaml` 使用 `corepack pnpm install --lockfile-only` 重新生成，确保移除 Fastify、Node Redis、ipaddr.js、Pino 和 server-only TypeScript 依赖；保留 web/build/test 和 ops 所需 Node 依赖。
- ESLint/Prettier glob 删除已移除的 server 配置引用，但保留 ops、tests、web 和 contracts。

执行：

```sh
corepack pnpm install --frozen-lockfile
cargo test --workspace --all-features
corepack pnpm typecheck
corepack pnpm lint
```

提交：`refactor: 移除 TypeScript 后端运行时`。

### 任务 23：执行完整验证矩阵

按顺序执行：

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
corepack pnpm format
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test:e2e
corepack pnpm compose:build
corepack pnpm compose:smoke
corepack pnpm performance:smoke
corepack pnpm backup:restore
corepack pnpm audit:runtime
corepack pnpm security:scan
git diff --check
```

验证重点：

- Rust 进程可以独立通过 live/ready，并托管 Svelte 页面。
- `/api/links` 成功、错误、challenge、限流、HEAD、404、503 行为与规格一致。
- 90 天 TTL、SET NX EX、Lua 计数器 TTL 和 risk TTL 有真实 Redis 证据。
- E2E 在 Chromium、mobile Chromium、WebKit 三个项目通过，最小宽度无横向溢出。
- 新 app 镜像无 Node runtime、非 root、只读根文件系统；Redis 无宿主机端口。
- 性能门槛仍为创建 p95 <= 100 ms、解析 p95 <= 50 ms、错误率 < 0.1%。
- 备份恢复仍校验 SHA-256、抽样 key 和 AOF baseline。

### 任务 24：执行一次性 Redis 数据集切换演练

只在完整验证通过、旧数据已备份后执行；该步骤是部署操作，不在 CI 中自动删除生产数据：

1. 使用 `./ops/redis-backup.sh` 保存旧数据和 checksum sidecar。
2. 停止旧 app 写入，保留旧 volume 和回滚镜像。
3. 用 `myurl-redis-data` 新卷启动 Redis 和 Rust app；确认 Rust app 只连接新 logical database/新卷。
4. 验证 `/health/live`、`/health/ready`、`POST /api/links`、`GET /:code`、`HEAD /:code`、未知 code 404。
5. 验证新创建的 key 只使用 `myurl:link:{code}` 等稳定 key，且旧 v2 数据不在新数据集内。
6. 演练回滚：停止新 app，保留新卷与备份，切回旧镜像和旧卷；不把两个版本的数据卷混用。
7. 记录切换时间、备份文件、checksum、验证结果和回滚结果；生产切换仍需独立授权。

验收完成后更新本计划状态和发布记录，提交：`chore: 完成 Rust 后端切换验证`。

## 11. 完成定义

只有下列条件全部满足，才可以删除旧 TypeScript server 或宣布迁移完成：

- `cargo fmt`、Clippy、Rust unit/API/Redis tests 全部通过。
- 前端 contracts、Svelte check/build 和现有三浏览器 E2E 全部通过。
- Compose build/smoke、性能、备份恢复、runtime audit 和 container scan 全部通过。
- Rust 服务不依赖 Node runtime；Node 只存在于 Svelte 构建、Playwright 和未迁移 ops 工具。
- 生产配置仍使用旧环境变量名，且生产挑战、HTTPS origin、secret、proxy CIDR 校验有效。
- 日志和浏览器错误响应没有泄露原始 IP、fingerprint、目标 URL、code、alias、Location、token、secret、Redis 错误文本或内部路径。
- 旧 v2 Redis 数据只在旧备份/旧卷中保留，新 Rust 数据集不双读旧数据。
- 公开 API 只使用稳定 `/api/links`，没有新增实现版本前缀。
- 运维脚本的保留是明确的第一阶段边界，不得被误报为“全部项目已没有 Node/shell”。
