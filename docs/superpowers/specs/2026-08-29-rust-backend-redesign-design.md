# MyURL Rust 后端重构设计规格

> 状态：待用户审阅
>
> 本规格描述第一阶段：使用 Rust 重写后端，保留 Svelte 前端的视觉和交互。它不把实现语言变化自动升级为公开 API 版本。

## 目标

- 使用 Rust 替换现有 TypeScript/Fastify 后端。
- 保留匿名短链的核心产品模型：创建、解析、固定 90 天有效期。
- 用稳定的无版本接口 `/api/links` 替换当前带版本号的创建路径。
- 保留 Redis、限流、Turnstile、目标地址安全校验、隐私日志和健康检查。
- 让 Rust 服务可以独立构建为一个生产容器，并继续托管 Svelte 构建产物。
- 将单元测试、HTTP 测试、Redis 集成、浏览器 E2E、Compose smoke、性能和安全验证纳入 Rust 迁移后的 CI。

## 非目标

- 第一阶段不把 Svelte 重写为 Rust/WASM 或 Rust SSR。
- 第一阶段不加入账号、管理后台、访问统计、密码保护、一次性链接或自定义过期时间。
- 第一阶段不引入 PostgreSQL 或其他关系型数据库。
- 第一阶段不迁移现有 v2 Redis 数据，也不提供旧数据双读。
- 第一阶段不重写 Node/shell 运维脚本；脚本只需适配新的服务入口和健康检查。

## 已确认决策

### 技术栈

后端采用以下 Rust 模块：

- `axum`：HTTP 路由和请求处理。
- `tokio`：异步运行时、超时和优雅关闭。
- `serde` 与 `serde_json`：JSON 请求与响应类型。
- `async-trait`：为可替换的异步 adapter 提供对象安全的 trait 接口。
- `redis`：Redis 连接、命令和 Lua 脚本调用。
- `thiserror`：领域错误和适配器错误的类型化表达。
- `tracing` 与 `tracing-subscriber`：结构化日志。
- `tower-http`：静态文件托管和 HTTP 层能力。

应用保持单体部署形态。HTTP adapter、短链领域模块、存储 adapter 和挑战验证 adapter 通过小接口连接。短链领域模块隐藏策略组合、风险判定、短码重试和错误映射，调用方只需要使用创建和解析接口。

### 公共入口

创建接口使用稳定路径：

```text
POST /api/links
```

不使用 `/api/v1`、`/api/v2` 或 `/api/v3` 这类实现版本路径。只有在未来出现无法兼容的公开协议时，才新增明确的兼容入口。

其他入口保持职责稳定：

```text
GET  /:code          -> 302 或 404/429/503 的浏览器响应
HEAD /:code          -> 302 或无响应体的错误响应
GET  /health/live    -> 200，不依赖 Redis
GET  /health/ready   -> 200 或 503，检查 Redis
```

### 产品规则

- 创建的短链固定有效 90 天。
- 目标地址只允许绝对 `http://` 或 `https://` URL。
- 目标地址拒绝用户名、密码、控制字符、空主机名、localhost/内部域名和被阻断的 IP 字面量。
- 可选别名为 4–32 个 ASCII 小写字母、数字、下划线或短横线；输入允许先转换为小写。
- `api`、`health`、静态资源路径等保留名称不能作为别名。
- 自动短码使用安全随机源生成 10 位 Base62 字符，并通过原子占位处理冲突。
- 创建和解析继续使用客户端指纹限流；服务端不记录原始 IP、完整指纹、目标地址、短码、别名、请求体、响应体或挑战 token。
- 风险达到阈值时要求 Turnstile；风险达到阻断阈值或计数器超限时返回限流错误。

## HTTP 契约

### 创建请求

```http
POST /api/links
Content-Type: application/json

{
  "url": "https://example.com/docs",
  "alias": "docs"
}
```

`alias` 可省略。第一阶段保留 `url` 字段名，避免把产品概念变化与后端迁移混在一起。请求体拒绝未知字段、过大的 URL、过大的 JSON 文档和非 JSON Content-Type。

### 创建成功响应

```http
HTTP/1.1 201 Created
Content-Type: application/json

{
  "code": "docs",
  "shortUrl": "https://myurl.example/docs",
  "expiresAt": "2026-11-27T12:00:00.000Z"
}
```

`shortUrl` 只使用启动配置中的可信公开 origin 生成。服务端不读取请求头来决定公开 origin。

### 错误响应

错误统一使用 `application/problem+json`，错误码由客户端用于展示本地化消息：

```json
{
  "type": "https://myurl.example/problems/alias-unavailable",
  "title": "Alias unavailable",
  "status": 409,
  "code": "alias_unavailable",
  "requestId": "req_abc123",
  "retryAfterSeconds": 10
}
```

`type` 是稳定的错误说明标识，`title` 是非本地化短标题，`status` 与 HTTP 状态一致，`code` 是机器可读业务码，`requestId` 必须存在。只有限流响应包含 `retryAfterSeconds` 和 `Retry-After` 响应头；挑战响应额外包含：

```json
{
  "challenge": {
    "provider": "turnstile",
    "siteKey": "public-site-key"
  }
}
```

第一阶段错误码与 HTTP 映射如下：

| 错误码 | HTTP 状态 | 含义 |
| --- | ---: | --- |
| `invalid_request` | 400 | JSON、Content-Type、字段或请求大小不合法 |
| `challenge_required` | 403 | 需要先完成人机验证 |
| `challenge_invalid` | 403 | 人机验证未通过 |
| `alias_unavailable` | 409 | 别名已占用或为保留名称 |
| `url_not_allowed` | 422 | 目标地址不符合策略 |
| `alias_invalid` | 422 | 别名格式不符合策略 |
| `rate_limited` | 429 | 风险或计数器达到阻断阈值 |
| `dependency_unavailable` | 503 | Redis 或挑战验证依赖不可用 |
| `code_generation_exhausted` | 503 | 自动短码在限定重试次数内无法占位 |

### 解析响应

已存在的短码返回：

```http
HTTP/1.1 302 Found
Location: https://example.com/docs
Cache-Control: no-store
Referrer-Policy: no-referrer
X-Robots-Tag: noindex, nofollow
```

`HEAD` 请求返回相同状态和响应头，但响应体为空。不存在或已过期的短码返回浏览器可读的 404 页面；解析限流返回 429；存储故障返回 503。浏览器错误页面不得泄露 Redis、内部异常或目标地址。

## 模块与接口

### HTTP adapter

职责：解析请求、执行 HTTP 层校验、提取客户端地址、调用短链领域模块、映射响应和设置安全头。它不实现别名规则、短码生成、风险判定或 Redis key 逻辑。

对外只暴露路由和 HTTP 响应。请求 ID 优先使用符合格式的调用方 ID，否则生成不可预测的新 ID。每个响应设置 `Cache-Control: no-store`、CSP、`Permissions-Policy`、`Referrer-Policy`、`X-Content-Type-Options`、`X-Frame-Options` 和 `X-Robots-Tag`。

### 短链领域模块

职责：编排创建和解析流程：

1. 使用密钥把客户端地址转换为限流指纹。
2. 原子递增创建计数器并读取风险分。
3. 根据计数、风险、挑战配置决定放行、挑战或阻断。
4. 标准化目标地址和别名。
5. 为别名或自动短码申请唯一占位。
6. 计算固定 90 天的过期时间并生成公开短链。
7. 对解析请求校验短码格式后读取目标地址。

领域模块依赖以下最小接口：

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

具体类型名可以在实现计划中按 crate 结构调整，但接口的职责和错误语义不变。`LinkStore` 的 Redis adapter 是生产实现，内存 adapter 只用于测试；挑战验证器分别提供 Cloudflare 实现和测试实现。

### 策略模块

策略模块保持纯函数优先，分别负责：

- URL 标准化和目标地址安全规则。
- 别名标准化、保留名称和格式检查。
- 安全随机短码生成与短码格式检查。
- 计数、风险分和挑战配置到风险决策的映射。
- 过期时间和 UTC 日期计算。

这些规则不访问网络、不创建 Redis 连接，也不写日志，因而可以直接使用 Rust 单元测试覆盖边界。

### Redis adapter

Redis adapter 负责：

- 链接占位：`SET key value NX EX ttl`。
- 解析读取：按稳定的业务 key 查找目标地址。
- 创建、解析计数器和风险分的原子递增及 TTL 设置。
- 命令超时、连接失败、结果形状错误到 `StoreError` 的映射。
- 连接关闭和健康检查。

代码不在 key 中加入公开 API 版本号。切换部署使用新的 Redis logical database 或全新数据卷；Rust 服务不会读取旧数据集，因此现有 v2 短链在新服务上不可解析。旧数据是否最终删除由运维切换步骤决定，删除前必须保留备份和回滚依据。

## 数据流

### 创建

```text
HTTP POST /api/links
  -> JSON 与请求头校验
  -> 客户端地址解析与 HMAC 指纹
  -> 创建计数器 + 风险读取
  -> 风险决策
       -> block: 429
       -> challenge 且无 token: 403 + challenge
       -> challenge 且 token 无效: 增加风险分，返回 403
  -> URL 策略校验
  -> 别名策略校验
  -> Redis 原子占位
       -> 别名冲突: 增加风险分，返回 409
       -> 自动短码冲突: 重试，耗尽返回 503
  -> 返回 code、shortUrl、expiresAt
```

计数器在策略决策前递增，确保尝试行为也受到限制。风险记录失败统一转换为依赖不可用，不允许静默忽略。

### 解析

```text
HTTP GET/HEAD /:code
  -> 客户端地址解析与 HMAC 指纹
  -> Redis 原子递增解析计数器
  -> 超限返回 429
  -> 短码格式校验
  -> Redis lookup
       -> 未命中返回 404
       -> 命中返回 302 + Location
```

解析阶段不抓取、探测或解析目标地址。Redis 中保存的目标地址只作为重定向 Location 返回。

### 健康检查

- `/health/live` 只证明进程和 HTTP 层可响应，不访问 Redis。
- `/health/ready` 在配置的超时时间内执行 Redis `PING`；成功返回 200，失败返回 503。
- 应用启动时解析并冻结配置；无效 origin、密钥、CIDR、限制关系或生产挑战配置导致启动失败。

## 配置与部署

- 保留现有环境变量名称，避免语言迁移扩大部署变更面。
- Rust 配置模块负责解析端口、公开 origin、Redis URL、超时、代理 CIDR、限流阈值、风险阈值和 Turnstile 配置。
- Docker 使用 Rust builder、Svelte builder 和最小运行时阶段；最终运行一个 Rust 二进制和 Svelte 静态产物。
- 应用容器继续以非 root、只读根文件系统、无额外 capability 运行；Redis 不发布宿主机端口。
- 容器健康检查调用 `/health/live`，Compose 仍等待 Redis healthy 后启动应用。
- 发布切换前先备份旧 Redis 数据，再让新应用指向新的 logical database 或新卷。切换后验证 live、ready、创建、跳转、HEAD 和 404；新应用不验证旧短链，因为旧数据不在新数据集内。

## 前端适配边界

Svelte 页面、组件、样式、Turnstile 加载时机、复制回退和页面状态机保持不变。只修改：

- `apps/web/src/lib/api.ts`：请求路径改为 `/api/links`，按 Problem Details 读取错误。
- `packages/contracts`：更新请求、成功响应、挑战和错误类型，使其与新契约一致；它在前端迁移完成前仍作为临时 TypeScript 类型层存在。
- 相关 E2E mock、API 路径断言和文档示例。

Rust 后端是请求/响应行为的唯一运行时实现；前端类型不能反过来决定服务端的安全规则。

## 错误处理与可观测性

- 领域错误、Redis 错误、挑战错误和 HTTP 校验错误必须使用显式类型；未知错误统一返回 `dependency_unavailable` 或对应 HTTP 层错误，不把内部异常文本返回给客户端。
- 请求日志只包含 request ID、路由模板、状态、耗时、业务结果分类和依赖分类。
- 成功创建日志只记录结果分类，不记录 code、alias、target URL 或 Location。
- Redis 和挑战超时必须有独立配置，并在响应中表现为 503。
- 关闭信号触发有上限的优雅关闭；超时后记录失败并以非零状态退出。

## 测试与验收

### Rust 单元测试

覆盖：

- URL 长度、控制字符、协议、凭据、内部域名和 IP 字面量规则。
- 别名大小写、ASCII、长度、字符集和保留名称。
- 短码字符集、长度、随机源边界和重试耗尽。
- 风险决策的放行、挑战、阻断优先级。
- 创建服务的别名成功、冲突、自动短码冲突、挑战和依赖失败。
- 固定 90 天过期时间和公开 URL 编码。
- 解析无效短码、未命中和存储失败。

### HTTP 集成测试

使用内存 adapter 测试：

- `/api/links` 的成功、错误 Content-Type、非法 JSON、未知字段、校验错误、挑战流程和限流响应。
- `GET/HEAD /:code` 的跳转、响应头、空 HEAD 响应体、404、429 和 503。
- live/ready 健康检查、安全响应头、request ID 和错误响应格式。
- 静态文件存在与不存在时的浏览器响应。

### Redis 与系统验证

- Redis 集成测试验证 `SET NX EX`、计数器 TTL、风险 TTL、重启后的数据行为和 PING 超时。
- 浏览器 E2E 验证创建、Enter 提交、别名冲突修正、复制回退、挑战重试、ready 降级和最小视口无横向溢出。
- Compose smoke 验证镜像启动、健康检查、创建、跳转、应用/Redis 重启后的新数据持久性和 Redis 无宿主机端口暴露。
- 性能测试保留创建 p95 不超过 100 ms、解析 p95 不超过 50 ms、错误率低于 0.1% 的门槛；基准结果以实际 Rust 镜像重新测量为准。
- 备份恢复验证针对新 Redis 数据集运行，确保备份校验、抽样解析和失败清理逻辑仍有效。
- CI 执行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test`、前端检查/构建、Playwright、Compose 和容器安全扫描。

## 分阶段切换

1. 建立 Rust crate、配置和领域模块，先用内存 adapter 完成纯 Rust 测试。
2. 加入 Redis/Turnstile adapter 和 Axum HTTP 层，完成新 `/api/links` 契约与 HTTP 集成测试。
3. 更新 Svelte 的 API client、contracts、E2E 和文档；在同一工作区验证前后端。
4. 更新 Docker、Compose、CI 和运维脚本，运行完整验证链。
5. 备份旧数据，让新部署使用新 logical database 或新卷，执行切换后的 smoke 和回滚演练。
6. Rust 服务稳定后删除 TypeScript 服务端源码与专属依赖；运维脚本是否迁移到 Rust 另立任务，不阻塞后端切换。

## 验收标准

- Rust 服务可独立构建、启动并通过 live/ready 检查。
- `/api/links`、`/:code`、健康检查和静态托管行为符合本规格。
- 固定 90 天 TTL、Redis 原子占位、限流、挑战、防 SSRF、隐私日志和安全响应头均有测试证据。
- Svelte 现有核心交互在新服务上继续通过 E2E。
- 新服务不读取旧 Redis 数据集，旧短链在新服务上返回 404。
- CI 的 Rust、前端、Redis、浏览器、Compose、性能、备份恢复和安全门禁全部通过。
- 切换步骤不在代码或文档中要求频繁修改公开 API 版本号；只有不兼容的公开协议变化才新增入口。
