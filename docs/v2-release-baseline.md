# v2 发布基线

## 阶段 0 记录

- v1 Git tag：`v1.13.0`
- v1 commit：`7dc3db6`
- v2 规格所在 master commit：`d63301f`
- v2 实施分支：`codex/myurls-v2`
- v2 工作树：`.worktrees/myurls-v2`
- v2 Redis volume：`myurl-v2-redis-data`
- v1 镜像命名：`ghcr.io/keleyaa/myurls:<verified-v1-tag-or-digest>`

v1 的 Go/Gin 运行时和 Redis 数据目录只作为回滚参考，不进入 v2 TypeScript 构建、测试、Docker context 或发布镜像。v2 Compose 使用独立 `app + redis` 服务和独立命名卷。

## 回滚点

1. 在 v2 生产切换前保留当前 v1 镜像 digest 与 v1 Redis 备份。
2. 发现 v2 应用问题时停止新写入并保留 v2 备份，不删除 v2 卷。
3. 按部署系统切回已验证的 v1 镜像或 v1 工作树；v1 只读取 v1 数据卷。
4. v2 恢复必须通过 `ops/redis-restore.sh` 恢复到新卷，并完成抽样验证后才重新启动 v2。

当前阶段只完成本地候选实现和验证，不执行合并、推送或生产流量切换。

## 候选验证摘要

2026-08-26 在本地 `codex/myurls-v2` 工作树执行 `corepack pnpm verify`，退出码为 0：

- 单元测试 69 个通过，覆盖率为 statements 99.05%、branches 97.46%、functions 100%、lines 99.02%。
- 真实 Redis 集成测试 6 个、API 合同测试 9 个、Chromium/Mobile Chromium/WebKit 端到端测试 24 个全部通过。
- 30 秒预热加 60 秒性能门禁通过：创建 p95 2.64ms、解析 p95 8.01ms，错误率均为 0。
- Compose 构建、带 Redis 密码的重启持久化、RDB 到新卷恢复演练、依赖审计和运行时外部资源扫描通过。
- Trivy HIGH/CRITICAL 容器漏洞门禁通过；本地候选没有执行生产发布或外部流量切换。
