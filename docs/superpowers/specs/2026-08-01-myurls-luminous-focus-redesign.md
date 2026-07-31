# MyUrls Luminous Focus 全新视觉设计规格

**日期：** 2026-08-01

**状态：** 已完成交互式头脑风暴并获用户批准

**分支：** `codex/myurls-luminous-focus-redesign`

## 1. 背景

当前公开页面已具备安全的短链创建、自动复制、手动复制、错误反馈和移动端布局，
但视觉仍是传统白色表单卡片：图片式 Logo 占据较多空间，两个输入框与两个按钮始终
可见，明暗主题、材质层级和品牌字体均不完整。

本次采用用户选择的 **A：Luminous Focus** 方向，从页面结构、品牌字、主题、反馈和
响应式布局重新设计，同时保持既有后端协议与安全边界。

## 2. 目标

1. 形成安静、现代、长期耐看的 Apple 风格单任务页面。
2. 把常用路径压缩为“输入链接 → 回车或单一主操作 → 自动复制”。
3. 默认隐藏自定义短码，只在用户明确需要时展开。
4. 使用可本地托管的开源字体重做 `MyUrls` 文字品牌。
5. 跟随系统自动切换明暗主题，不增加主题按钮。
6. 保留完整的键盘、屏幕阅读器、减少动态、减少透明度与高对比度支持。
7. 在页面底部以静态形式展示 GitHub 仓库归属，不引入 GitHub API。

## 3. 非目标

- 不修改 `/short` 请求或短链跳转协议。
- 不修改 Redis、鉴权、限流、部署或后端配置。
- 不增加历史记录、二维码、统计、分享菜单或账号系统。
- 不增加主题切换、实时 Star/Fork、外部分析或遥测。
- 不引入前端框架、动画库、图标库或运行时 CDN。

## 4. 已批准的产品决策

| 决策 | 结果 |
| --- | --- |
| 视觉方向 | A：Luminous Focus |
| 主题 | 跟随系统自动切换明暗主题 |
| 品牌拼写 | `MyUrls`，保留项目名一致性 |
| 默认表单 | 只展示长链接输入与一个内嵌主操作 |
| 自定义短码 | 使用原生 `details/summary` 按需展开 |
| 复制 | 成功后自动复制；整个结果表面可再次复制 |
| GitHub 展示 | A：静态仓库标识，无 API 请求 |
| 字体 | Manrope 品牌字，本地托管，中文使用系统字体 |

## 5. 页面结构

页面维持单列、单焦点结构，最大内容宽度约 `590px`：

1. **品牌区**：居中的文字品牌 `MyUrls.`，句点使用动作蓝；副标题为
   “把长链接，变得简单。”
2. **主工具表面**：单层半透明材质，内部只在首屏展示长链接输入与右侧箭头提交。
3. **可选短码**：工具表面内的 `summary` 文本“自定义短码”，展开后显示规则说明和
   输入框。
4. **结果表面**：初始隐藏；成功后在工具表面内部出现，显示生成地址与复制反馈，
   整个表面是一个可聚焦按钮。
5. **状态区**：使用 `role="status"` 与 `aria-live="polite"`，显示空闲、生成中、成功或
   错误状态。
6. **仓库页脚**：主工具下方保持足够留白，显示内联 GitHub SVG、
   `keleyaa/MyUrls`、`Go · MIT` 与外链箭头。仓库只在此出现，不保留重复的顶部入口。

旧 `public/logo.png` 不再被页面或测试引用，并从仓库删除。

## 6. 组件职责

### 6.1 品牌区

- `h1` 必须包含真实文本 `MyUrls`，不使用图片或背景图模拟文字。
- Manrope 仅用于英文品牌；中文副标题与所有表单文字使用系统字体栈。
- 品牌字使用大字号、紧行高与负 tracking；字号通过 `clamp()` 响应式缩放。

### 6.2 主输入与提交

- 保留 `id="long-url"`、`name="longUrl"`、`type="url"`、`autocomplete="url"`。
- `id="shorten-button"` 是页面唯一主操作，视觉上内嵌于输入右侧。
- 按钮必须有可访问名称“生成短链接”；箭头只作视觉符号。
- 表单原生 Enter 提交与点按箭头完全等价。
- 指针按下立即缩放反馈；生成期间显示 spinner 并禁用重复提交。

### 6.3 自定义短码

- 使用原生 `details`，默认关闭，不另写弹窗或浮层状态机。
- 保留 `id="short-key"`、`name="shortKey"`、`maxlength="64"` 与既有字符约束。
- 展开与收起沿相同路径，且不阻断表单键盘顺序。

### 6.4 结果与复制

- 结果以 `button id="copy-button"` 表示，初始隐藏且禁用。
- 生成地址放在按钮内部的 `span id="short-url"`，长文本使用单行省略，不造成页面溢出。
- 成功后先尝试自动复制，再显示结果；结果出现后按钮可再次复制。
- 兼容复制不再依赖可见的只读输入框：当 Clipboard API 不可用时，临时创建只读
  `textarea`、选择、调用 `document.execCommand('copy')`，随后立即移除。
- 若两种复制方式都失败，结果仍保持可见，状态提示用户手动选择地址。

### 6.5 GitHub 页脚

- 使用一个语义明确的 `<a>`，目标固定为 `https://github.com/keleyaa/MyUrls`。
- 使用 `target="_blank"`、`rel="noopener noreferrer"`。
- GitHub Mark 使用内联 SVG，不加载图标库或远程图片。
- `Go · MIT` 是静态项目标识；不展示会过时的 Star/Fork 数字，不调用 GitHub API。

## 7. 数据流与状态

后端数据流保持不变：

```text
用户提交
  → 客户端 URL 与表单约束校验
  → POST /short（FormData: longUrl, shortKey）
  → 校验 HTTP 成功且 payload.Code === 1、ShortUrl 为非空字符串
  → 自动复制
  → 展示结果与最终状态
```

前端有六个明确状态：

1. `idle`：提示“粘贴链接后按回车，或点按箭头”。
2. `invalid`：聚焦错误字段并给出具体输入提示。
3. `loading`：提交禁用、spinner 可见、`aria-busy="true"`。
4. `success`：结果可见，自动复制成功时提示“已生成并自动复制”。
5. `request-error`：结果隐藏，显示稳定且不泄露后端内部信息的通用错误。
6. `copy-error`：结果保留，提示“已生成，请手动复制”。

新提交开始时必须清除旧结果与旧复制状态；无论成功失败，`finally` 都恢复提交能力。

## 8. 视觉系统

### 8.1 主题

- 默认浅色 token；使用 `prefers-color-scheme: dark` 覆盖深色 token。
- 不提供手动主题切换，也不保存主题状态。
- 浅色：Cloud Light 背景、蓝色与淡紫环境光、白色半透明工具表面。
- 深色：Night Ink 背景、低饱和蓝紫环境光、深蓝灰半透明工具表面。
- 主题切换仅过渡颜色与透明度，避免整页位移或亮度突变。

### 8.2 字体

- 从 Manrope 官方开放字体发行物取得用于 Latin 的可变 `woff2`，随项目本地提供。
- 在字体目录保留 SIL Open Font License 1.1 文本。
- `@font-face` 只引用同源相对 URL，设置 `font-display: swap`、合法 weight range 和
  normal style。
- 正文栈为 `-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`；中文不强行使用
  缺少中文字形的 Manrope。

### 8.3 材质与层级

- 全页只使用一层主玻璃材质，不在半透明表面上继续叠加第二层玻璃。
- 工具表面使用亮边、轻描边、环境阴影和 `backdrop-filter` 建立悬浮层级。
- 输入与结果是较实的功能表面，确保文字对比度不依赖背景。
- 背景环境光只使用 CSS 渐变，不加载图片。

### 8.4 动效

- 按下反馈：`90–120ms`，立即缩放，释放后从当前视觉值恢复。
- hover/focus：约 `160–200ms`，只改变 transform、颜色、阴影或透明度。
- 结果出现：`280–320ms`，无回弹，组合轻微位移、scale、opacity 与 blur。
- 不锁定与当前状态无关的输入；用户可以在反馈期间继续聚焦和编辑。
- `prefers-reduced-motion: reduce` 时移除位移、scale、spinner 旋转之外的非必要运动，
  结果改为短淡入或即时显示。
- `prefers-reduced-transparency: reduce` 时使用近实色工具表面并关闭 backdrop blur。
- `prefers-contrast: more` 时提高表面不透明度、描边和焦点对比度。

## 9. 响应式与无障碍

- 桌面目标视口 `1440×900`，移动目标视口 `390×844`。
- 页面宽度不低于 `320px`；移动端保持不小于 `16px` 的安全边距。
- 主操作触控区域桌面 `50px`、移动端至少 `44px`。
- 所有交互必须可用键盘访问并具有 `:focus-visible` 样式。
- `details/summary`、表单标签、状态区和结果按钮使用原生语义，不以 `div` 冒充控件。
- 动态字号放大时使用 `rem`、`em` 与弹性布局，结果文本省略但完整值保留在可访问名称中。
- 不允许页面级横向滚动或交互控件重叠。

## 10. 错误与安全边界

- 客户端只接受首尾无空格、协议为 HTTP/HTTPS 且存在 hostname 的 URL。
- 自定义短码继续依赖浏览器约束与后端权威校验。
- 网络失败、非 2xx、业务失败或响应结构错误统一进入安全的通用错误状态。
- 不把后端内部错误文本直接写入页面。
- 不记录用户输入，不增加遥测，不把链接发送给第三方。
- 页面运行时资源全部同源；唯一外部 URL 是用户主动点击的 GitHub 导航链接。

## 11. 文件范围

预计实现只修改以下范围：

- `public/index.html`：新语义结构、文字品牌、内联 GitHub SVG、精简表单。
- `public/styles.css`：主题 token、布局、材质、响应式与无障碍媒体查询。
- `public/app.js`：明确状态、结果表面复制、临时 textarea fallback。
- `public/fonts/`：Manrope Latin 可变字体及 OFL 许可证。
- `public/logo.png`：删除。
- `server_test.go`：更新静态资源、本地字体、语义和禁止外部依赖的断言。
- `tests/e2e/app.spec.js`：更新展开、提交、自动复制、再次复制和错误状态用例。
- `playwright.config.js`：覆盖桌面/移动与浅色/深色组合。

不修改 Go 业务实现、Redis 逻辑或 Compose 配置。

## 12. 测试设计

### 12.1 Go 静态资源契约

- 首页、CSS、JS、字体均为同源静态资源并返回正确 Content-Type。
- 页面没有外部 stylesheet、script、font、image 或运行时 fetch。
- 允许固定 GitHub 导航 URL，但禁止 GitHub API、Google Fonts、unpkg、jsDelivr 等运行时依赖。
- Manrope `@font-face` 只引用本地路径，并存在 OFL 许可证。
- 旧 `logo.png` 引用和路由断言删除。
- 验证核心语义 ID、label、`details/summary`、状态区、结果按钮和安全外链属性。
- 验证明暗、减少动态、减少透明度、高对比度媒体查询与明确的焦点样式。

### 12.2 JavaScript 行为

- HTTP/HTTPS URL 校验保持覆盖。
- Enter 与主按钮走同一提交路径。
- 加载期间按钮禁用、文案/状态正确且不会重复提交。
- 成功后结果出现并自动复制；结果按钮可再次复制。
- Clipboard API 与临时 textarea fallback 均覆盖。
- 请求失败与复制失败进入不同状态，且复制失败不丢失结果。

### 12.3 Playwright

- 使用四个项目：Desktop Light、Desktop Dark、Mobile Light、Mobile Dark。
- 每个项目验证品牌、默认折叠状态、展开短码、生成、自动复制、再次复制和业务错误。
- 检查 loading 状态、`aria-busy`、控制台错误、page error、页面横向溢出和控件重叠。
- 保存四种成功态截图并进行原始分辨率目视检查。

### 12.4 完整门禁

- `gofmt`、`go test -count=1 ./...`、shuffle、race、vet、govulncheck。
- `npm outdated --json` 与 Playwright 全项目。
- Docker 镜像构建及首页/健康检查 smoke test。
- `git diff --check` 与外部资源扫描。

## 13. 验收标准

1. 默认首屏只出现一个长链接输入和一个内嵌主操作。
2. 自定义短码默认折叠，展开后可完整使用。
3. Enter 与箭头均能生成；成功后自动复制，结果表面可再次复制。
4. `MyUrls.` 使用本地 Manrope 文字品牌，旧图片 Logo 不再存在。
5. 页面跟随系统正确呈现浅色与深色，无手动主题按钮。
6. GitHub 静态仓库标识位于底部，且页面不调用 GitHub API。
7. 桌面、移动、减少动态、减少透明度和高对比度均可用。
8. 无外部运行时资源、无控制台错误、无页面横向溢出或控件重叠。
9. 后端协议、现有安全控制与所有完整门禁保持通过。
