# MyUrls 日志与时区加固实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 隐藏短码与客户端标识，抑制成功健康检查日志，统一 `+08:00` 时间，并限制容器日志增长。

**架构：** 在 HTTP 中间件边界生成隐私安全的路由字段，在 Zap 编码器边界统一时间，在镜像与 Compose 边界固定时区和轮转。业务 API 与 Redis 数据结构不变。

**技术栈：** Go、Gin、Zap、Docker、Docker Compose、Testify

---

### 任务 1：访问日志隐私契约

**文件：**
- 修改：`logger_test.go`
- 修改：`logger.go`
- 修改：`server_test.go`

- [ ] **步骤 1：编写失败测试**，要求真实短码记录为 `/:shortKey`，未匹配路径为 `unmatched`，成功 `/healthz` 不记录，503 健康检查保留。
- [ ] **步骤 2：运行 `go test -run 'TestPrivacySafeRoute|TestShouldLogRequest|TestServiceLogger' ./...`，确认因缺少行为而失败。**
- [ ] **步骤 3：实现 `privacySafeRoute`、`shouldLogRequest` 和最小化访问日志字段。**
- [ ] **步骤 4：重新运行目标测试并确认通过。**

### 任务 2：中国标准时间契约

**文件：**
- 修改：`logger_test.go`
- 修改：`logger.go`
- 修改：`Dockerfile`

- [ ] **步骤 1：编写失败测试**，要求编码后的日志时间带 `+08:00`。
- [ ] **步骤 2：运行目标测试确认当前 UTC 编码失败。**
- [ ] **步骤 3：加入固定中国时区编码器，并在最终镜像复制 `/usr/share/zoneinfo/Asia/Shanghai`、设置 `TZ=Asia/Shanghai`。**
- [ ] **步骤 4：运行日志测试和镜像构建确认通过。**

### 任务 3：容器日志轮转与文档

**文件：**
- 创建：`deployment_test.go`
- 修改：`docker-compose.yaml`
- 修改：`README.md`
- 修改：`docs/operations.md`

- [ ] **步骤 1：编写失败测试**，要求两个服务使用 `json-file`、`max-size: 10m`、`max-file: 3`。
- [ ] **步骤 2：运行部署测试确认 Compose 当前缺少轮转配置。**
- [ ] **步骤 3：添加轮转配置，并说明日志字段、时区、健康检查抑制和敏感信息边界。**
- [ ] **步骤 4：运行 `gofmt`、`go vet ./...`、`go test ./...`、`docker compose config --quiet` 和镜像构建。**
