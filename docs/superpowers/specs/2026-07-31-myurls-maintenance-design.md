# MyUrls 兼容性优先维护设计

**日期：** 2026-07-31

**状态：** 已批准

**分支：** `codex/myurls-maintenance`

## 1. 背景与目标

MyUrls 当前代码量较小、功能集中，但首轮审核确认存在以下维护风险：

- 后端接受任意跳转协议，前端校验无法构成安全边界。
- 自定义短码采用 `EXISTS` 后 `SETEX` 的非原子写入，并发时可能覆盖。
- 测试共享全局 Redis 客户端且依赖本机 Redis，默认完整测试会失败。
- Go、Go 模块、Redis、容器镜像和 GitHub Actions 已落后于稳定版本。
- 内置页面依赖已停止维护的 Vue 2 生态及多个运行时 CDN。
- HTTP 服务缺少超时、健康检查、优雅退出及可用的发布质量门禁。

本轮目标是在保持现有使用方式兼容的前提下，把 MyUrls 维护为可测试、
可安全部署、依赖版本明确且适合后续接入 Subweb 统一编排的短链服务。

## 2. 范围

### 2.1 本轮包含

- 保持现有 CLI、环境变量、`POST /short`、`GET /:shortKey` 和内置页面兼容。
- 后端 URL 与短码校验、请求大小限制、可选鉴权和可配置限流。
- Redis 原子写入、碰撞处理和错误分类。
- HTTP 超时、健康检查、优雅退出和日志运行基线。
- Go、全部可达 Go 模块、Redis、Docker 镜像和 GitHub Actions 升级。
- 用原生 HTML、CSS、JavaScript 替换全部浏览器框架和 CDN 依赖。
- Docker、Compose、CI、镜像发布、升级与回滚文档维护。

### 2.2 本轮不包含

- 不把 Subweb 或 SubConverter-Extended 源码合入本仓库。
- 不实现管理后台、访问统计、二维码、自定义域名管理或用户系统。
- 不引入 API v2，不修改成功响应字段，不迁移到 Redis 之外的存储。
- 不默认启用 API Token，也不改变短链默认 `301` 重定向语义。

## 3. 兼容性契约

以下行为必须由自动化契约测试固定：

1. 原有 CLI 参数 `-port`、`-domain`、`-proto`、`-conn`、`-password` 和
   `-h` 保留，现有默认值不变。
2. 原有环境变量 `MYURLS_PORT`、`MYURLS_DOMAIN`、`MYURLS_PROTO`、
   `MYURLS_REDIS_CONN`、`MYURLS_REDIS_PASSWORD` 保留，环境变量仍覆盖参数。
3. `POST /short` 同时接受表单和 JSON，字段仍为 `longUrl`、`shortKey`。
4. 普通 URL 和旧客户端发送的标准 Base64 URL 均可创建短链。
5. 成功响应保持 `{"Code":1,"ShortUrl":"<url>"}`。
6. 业务错误保持 `Code`、`Message`、`Data` 字段及现有业务码。
7. 业务参数错误和短码冲突继续使用 HTTP 200，避免破坏旧客户端。
8. `GET /:shortKey` 成功时继续返回 301，短码不存在时返回 404。
9. 页面继续支持输入 URL、回车或按钮提交、展示短链、自动复制、手动复制
   和移动端布局。

新增的拒绝行为限定为：非 HTTP(S) URL、无主机 URL、包含用户凭据的 URL、
含控制字符的 URL、非法或保留短码、超过请求上限的请求。可选鉴权或限流仅在
部署者显式配置后生效。

## 4. 组件边界

为保持小项目的可读性，同时解除当前职责耦合，采用以下文件边界：

- `main.go`：解析配置、初始化依赖、处理进程信号和启动服务。
- `config.go`：集中定义参数、环境变量、默认值及配置校验。
- `server.go`：创建 Gin Router、`http.Server`、超时和优雅退出。
- `handlers.go`：HTTP 绑定、兼容响应、状态码和重定向。
- `validation.go`：URL、Base64 兼容输入和短码校验。
- `logic.go`：短链创建、随机短码重试、查询和领域错误。
- `redis.go`：Redis 客户端生命周期和原子存储操作。
- `middleware.go`：可选 Bearer Token、请求大小和全局写入限流。
- `health.go`：`/healthz` 与二进制容器健康检查模式。
- `public/index.html`、`public/app.js`、`public/styles.css`：无框架页面。

每个新增生产文件配套同名或同职责测试文件；不创建通用工具包或内部框架。

## 5. 创建短链的数据流

`POST /short` 按以下固定顺序处理：

1. 鉴权中间件在配置 Token 时校验 `Authorization: Bearer <token>`。
2. 限流中间件在配置速率时消费一个全局写入令牌。
3. 请求体读取上限默认为 16 KiB，超限返回参数错误业务结构。
4. Gin 绑定表单或 JSON 到现有 `LongToShortParams`。
5. 先把 `longUrl` 作为普通 URL 校验；若失败，再尝试标准 Base64 解码并
   校验解码结果。两者均失败时返回参数错误。
6. 自定义短码必须匹配 `[A-Za-z0-9_-]{1,64}`，并拒绝 `healthz`、
   `logo.png`、`app.js`、`styles.css` 等固定路由名。
7. 未提供短码时使用 `crypto/rand` 从现有字母数字字符集生成 7 位短码。
8. Redis 通过一条带 `NX` 与一年 TTL 的 `SET` 命令原子写入。
9. 自定义短码冲突立即返回现有冲突消息；自动短码冲突最多重新生成 5 次。
10. 成功后按现有 `proto://domain/shortKey` 规则生成兼容响应。

随机源读取失败、自动碰撞达到上限或 Redis 错误均记录内部错误，但响应不得
泄露 Redis 地址、密码、堆栈或底层错误文本。

## 6. 查询与错误处理

`GET /:shortKey` 在存储层保留完整错误信息：

- 命中：返回已有长 URL，使用 301 重定向。
- `redis.Nil`：返回 404 和现有“短链不存在或已过期”消息。
- 其他 Redis 错误：返回 500，日志记录结构化内部原因。

`/healthz` 对 Redis 执行有超时的 `PING`：连接正常返回 200 和
`{"status":"ok"}`，不可用返回 503 和 `{"status":"unavailable"}`。
健康响应不包含连接配置。

## 7. 安全与服务配置

新增配置及固定默认值如下：

| 环境变量 | 默认值 | 行为 |
| --- | ---: | --- |
| `MYURLS_API_TOKEN` | 空 | 空值关闭鉴权；非空时保护 `POST /short` |
| `MYURLS_RATE_LIMIT_RPS` | `0` | `0` 关闭；正数限制每秒全局创建速率 |
| `MYURLS_RATE_LIMIT_BURST` | `10` | 限流桶突发容量 |
| `MYURLS_MAX_BODY_BYTES` | `16384` | `/short` 请求体上限 |
| `MYURLS_READ_HEADER_TIMEOUT` | `5s` | HTTP 请求头超时 |
| `MYURLS_READ_TIMEOUT` | `10s` | HTTP 读取超时 |
| `MYURLS_WRITE_TIMEOUT` | `10s` | HTTP 写入超时 |
| `MYURLS_IDLE_TIMEOUT` | `60s` | HTTP 空闲连接超时 |
| `MYURLS_SHUTDOWN_TIMEOUT` | `10s` | 优雅退出上限 |

限流使用 `golang.org/x/time/rate v0.15.0`，只限制创建短链，不限制首页、
健康检查或短链跳转。直接运行二进制时鉴权与限流默认关闭；仓库提供的 Compose
示例默认设置每秒 5 次、突发 10 次，部署者可显式改为 0。

服务接收 SIGINT/SIGTERM 后停止接收新请求，在退出上限内完成在途请求，随后
关闭 Redis 客户端并同步日志。删除过时的 1 GiB GC ballast。

## 8. 依赖与可重复构建

版本基线来自
[依赖版本研究](../research/2026-07-31-dependency-versions.md)，只使用稳定版本：

- `go 1.25.0` 表示升级后依赖图的最低版本。
- `toolchain go1.26.5` 与 `golang:1.26.5-alpine3.24` 使用最新修补编译器。
- Gin `v1.12.0`、go-redis `v9.21.0`、Testify `v1.11.1`、Zap
  `v1.28.0`、miniredis `v2.38.0`、Lumberjack `v2.2.1`。
- 新增 `golang.org/x/time v0.15.0`。
- 间接依赖不逐个强制覆盖；先更新直接图并 tidy，再执行第二阶段
  `go get -u all` 与 tidy，以 Go 最小版本选择解析全部可达模块。

完成后 `go list -m -u all` 不得报告可升级的可达稳定模块。Docker 镜像使用
精确版本和实施时重新解析的多架构 digest；Actions 使用对应最新稳定 tag 的
完整提交 SHA，并在行尾标注可读版本。

增加 Dependabot，按周检查 `gomod`、`docker` 和 `github-actions`。自动更新只
创建 PR，不绕过测试或自动合并。

## 9. 无框架页面

页面不迁移到 Vue 3，而是删除 Vue、Element UI、Axios 和 vue-clipboard2：

- `fetch` + `FormData` 调用现有 `/short`。
- 直接发送普通 URL；后端继续支持旧客户端 Base64 输入。
- 优先使用 `navigator.clipboard.writeText`，仅在不可用时使用受控回退。
- 保留加载禁用、成功/失败反馈、回车提交和移动端宽度。
- 不从 CDN 加载脚本、样式、字体或图标，页面可离线随镜像提供。
- 表单控件包含显式 label、焦点样式、键盘操作和状态公告区域。

页面视觉保持克制的工具界面，不进行品牌重设计，不添加营销区或说明卡片。

## 10. Docker 与 Compose

Dockerfile 使用多阶段构建，最终镜像保持最小化并使用数值非 root 用户
`65532:65532`。构建阶段创建并赋予 `/app/logs` 正确所有权；静态资源和二进制
只读复制。应用增加 `-healthcheck` 模式，使 scratch 运行时无需 curl 或 shell
即可执行容器 `HEALTHCHECK`。

Compose 做以下调整：

- 删除废弃的顶层 `version`。
- Redis 固定为 `redis:8.10.0` 及 digest，启用持久卷和 `redis-cli ping`。
- MyUrls 等待 Redis 健康后启动，只暴露应用端口，不暴露 Redis。
- 增加 `read_only`、`no-new-privileges`、`cap_drop: [ALL]` 和日志卷。
- 使用 `.env.example` 记录全部配置，示例默认启用 5 RPS/10 burst。
- 增加 `.dockerignore`，排除 `.git`、构建产物、日志、数据和本地环境文件。

Redis 7 升级前必须停止写入并备份持久卷。升级后验证 RDB/AOF 加载、创建、查询
和重启持久化。回滚 Redis 7 时恢复升级前备份，不复用 Redis 8 写入后的数据卷。
应用存储命令保持 Redis 7 可支持的基础命令，不把 Redis 8 作为协议最低要求。

## 11. CI 与镜像发布

普通 CI 对 push 和 pull request 执行：

1. Go 格式检查、`go vet`、单元及 HTTP 测试。
2. 随机测试顺序重复运行和 `go test -race ./...`。
3. 固定版本 `govulncheck ./...`。
4. Linux amd64/arm64、Windows amd64 和现有支持目标的交叉构建。
5. Docker 构建、非 root 检查、健康检查和 Redis 8 冒烟测试。
6. 原生页面的桌面与移动端浏览器流程验证。

Actions 升级到研究记录中的稳定版本线，并以 SHA 固定。发布工作流只在版本 tag
或手动触发时运行，使用 `GITHUB_TOKEN` 发布多架构镜像到
`ghcr.io/keleyaa/myurls`，同时生成提交 SHA tag、版本 tag 和 digest。Docker Hub
不再是默认发布门禁，避免缺失外部 secrets 导致普通维护分支失败。

## 12. 测试策略

所有行为变更遵循红灯、绿灯、重构顺序：

- 配置测试：默认值、参数优先级、环境覆盖、非法配置。
- 校验测试：HTTP(S)、Base64、用户凭据、控制字符、短码边界和保留名。
- 存储测试：NX 原子语义、TTL、Redis 错误和不存在分类。
- 并发测试：同一自定义短码并发创建只能有一个成功。
- Handler 契约测试：表单、JSON、旧 Base64、响应字段和 HTTP 状态。
- 中间件测试：Token 开关、Bearer 校验、请求上限、限流开关和 429。
- 服务测试：健康检查、超时配置、关闭流程和启动错误。
- 页面测试：提交、加载、结果、剪贴板、错误反馈、桌面和移动端布局。
- 容器测试：非 root、健康状态、Redis 8 持久化、Compose 重启恢复。

miniredis 用于快速单元测试，真实 Redis 8 容器用于集成与并发语义验证。测试不得
依赖开发机已有 Redis、固定执行顺序或残留全局客户端。

## 13. 实施与提交顺序

实现计划按以下可独立验证的提交边界拆分：

1. 隔离测试 Redis 与全局状态。
2. 升级 Go 工具链和全部 Go 模块。
3. 增加 URL、Base64 和短码校验。
4. 改用安全随机与 Redis 原子创建。
5. 增加可选鉴权、请求上限和限流。
6. 重构服务器生命周期、健康检查和错误分类。
7. 将页面替换为无框架实现。
8. 加固 Docker、Compose 和 Redis 8 升级流程。
9. 重建 CI、漏洞检查和 GHCR 发布。
10. 同步 README、配置、运维和回滚文档。
11. 执行全量回归、浏览器、容器和发布前验证。

每个提交只包含对应任务文件及测试。当前已存在但未提交的 miniredis 测试改动
归入第 1 项，在实现计划审查后重新验证，不与规格提交混合。

## 14. 验收标准

维护版本只有同时满足以下条件才可进入分支收尾：

- 本设计第 3 节的全部兼容契约通过自动化测试。
- 危险/无效输入、非法短码和超大请求均被后端拒绝。
- 100 个并发同键创建中只有一个写入成功，已有值不被覆盖。
- 单元、HTTP、随机顺序、竞态、真实 Redis 8 和浏览器测试全部通过。
- `go vet`、`govulncheck`、全部目标构建和 `go list -m -u all` 通过。
- 镜像以非 root 运行，健康检查、Compose 重启和 Redis 持久化通过。
- 页面不产生外部网络依赖，不存在控制台错误或布局重叠。
- README 中的本地、Docker、Compose、升级、回滚命令均被实际执行验证。
- 工作树只包含计划内文件，任何提交和推送前再次核对远端为
  `https://github.com/keleyaa/MyUrls.git`。
