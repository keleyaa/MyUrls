# MyUrls Luminous Focus 重设计实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用
> superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）
> 语法来跟踪进度。

**目标：** 将 MyUrls 首页重做为自动适配明暗主题、单一主操作、可访问且无外部运行时依赖的 Luminous Focus 页面。

**架构：** 保持 `/short` 后端协议和原生 HTML/CSS/JavaScript 架构不变；Gin 只增加本地 Manrope 字体路由并移除旧 Logo 路由。页面用语义 HTML 表达单任务流程，用 CSS token 和媒体查询承载视觉适配，用六态 JavaScript 状态机处理校验、请求、复制与失败恢复。

**技术栈：** Go 1.26.5、Gin 1.12、原生 HTML/CSS/JavaScript、Manrope variable WOFF2、Playwright 1.62.1、Docker Compose。

**设计规格：**
[`docs/superpowers/specs/2026-08-01-myurls-luminous-focus-redesign.md`](../specs/2026-08-01-myurls-luminous-focus-redesign.md)

---

## 执行约束

- 工作目录固定为 `/Users/li/Desktop/GitHub/MyUrls`。
- 分支固定为 `codex/myurls-luminous-focus-redesign`。
- 本机没有 Go；所有 Go 命令使用下方固定的 Go 1.26.5 容器。
- 不修改 `/short` 协议、短链跳转、Redis、鉴权、限流、Compose 生产配置或 Go 业务逻辑。
- 不增加前端框架、图标库、动画库、主题开关、GitHub API、遥测或运行时 CDN。
- 每个生产行为先写失败测试并看到预期红灯，再写最少实现；绿灯后才能提交。
- 每次提交前运行 `git diff --check`，并确认暂存区只包含当前任务文件。
- 不推送分支、不创建 PR，除非用户另行授权。

每个新 shell 会话先定义以下只读辅助函数；后续 `go_docker` 命令均调用它：

```sh
go_docker() {
  docker run --rm \
    -v /Users/li/Desktop/GitHub/MyUrls:/app \
    -v myurls-go-mod:/go/pkg/mod \
    -v myurls-go-build:/root/.cache/go-build \
    -w /app \
    golang:1.26.5-alpine3.24@sha256:0178a641fbb4858c5f1b48e34bdaabe0350a330a1b1149aabd498d0699ff5fb2 \
    "$@"
}
```

## 文件结构

### 新建

- `public/fonts/manrope-latin-wght-normal.woff2`：只含 Latin 的 Manrope 200–800 可变字重网页字体。
- `public/fonts/OFL.txt`：Manrope 的 SIL Open Font License 1.1 原文。

### 修改

- `server.go`：注册本地 WOFF2 静态路由，删除旧 `logo.png` 静态路由。
- `server_test.go`：拆分字体、HTML、CSS 和 JavaScript 静态资源契约。
- `public/index.html`：文字品牌、单输入主操作、折叠短码、结果按钮和静态 GitHub 页脚。
- `public/styles.css`：Luminous Focus token、主题、材质、动效、响应式和无障碍媒体查询。
- `public/app.js`：六态 UI、请求生命周期、自动复制、结果再次复制和 textarea fallback。
- `tests/e2e/app.spec.js`：确定性的交互、错误、复制、布局和截图测试。
- `playwright.config.js`：桌面/移动与浅色/深色四个 Chromium 项目。

### 删除

- `public/logo.png`：旧图片品牌不再参与页面或路由。

---

### 任务 1：本地托管 Manrope 字体

**文件：**
- 创建：`public/fonts/manrope-latin-wght-normal.woff2`
- 创建：`public/fonts/OFL.txt`
- 修改：`server.go`
- 修改：`server_test.go`

- [ ] **步骤 1：编写本地字体红灯测试**

在 `server_test.go` 的 import 中增加 `os`，并添加：

```go
func TestManropeFontIsServedLocally(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})

	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(
		http.MethodGet,
		"/fonts/manrope-latin-wght-normal.woff2",
		nil,
	))

	require.Equal(t, http.StatusOK, response.Code)
	assert.Contains(t, response.Header().Get("Content-Type"), "font/woff2")
	assert.True(t, strings.HasPrefix(response.Body.String(), "wOF2"))

	license, err := os.ReadFile("public/fonts/OFL.txt")
	require.NoError(t, err)
	assert.Contains(t, string(license), "SIL OPEN FONT LICENSE Version 1.1")
}
```

- [ ] **步骤 2：运行测试验证字体缺失红灯**

运行：

```sh
go_docker go test -run '^TestManropeFontIsServedLocally$' -count=1 ./...
```

预期：FAIL；字体请求不是 `200`，且 `public/fonts/OFL.txt` 不存在。

- [ ] **步骤 3：下载固定的 Latin WOFF2 与许可证并核验**

运行：

```sh
mkdir -p public/fonts
curl -fL 'https://fonts.gstatic.com/s/manrope/v20/xn7gYHE41ni1AdIRggexSvfedN4.woff2' -o public/fonts/manrope-latin-wght-normal.woff2
curl -fL 'https://raw.githubusercontent.com/google/fonts/8f9a401dbb3793e0d1264b15d96aa253f05280f5/ofl/manrope/OFL.txt' -o public/fonts/OFL.txt
shasum -a 256 public/fonts/manrope-latin-wght-normal.woff2 public/fonts/OFL.txt
```

预期校验值：

```text
e310b55a7fd9677f5e3555e6c6c4d064fa1f1d24393f0ddbe217cea12a8c432f  public/fonts/manrope-latin-wght-normal.woff2
e01b637272e0cbdfb240184dd98ea5cc671556d9894dae2668d92ab2c906787c  public/fonts/OFL.txt
```

- [ ] **步骤 4：注册同源字体路由**

在 `server.go` 的静态文件注册区增加：

```go
router.StaticFile(
	"/fonts/manrope-latin-wght-normal.woff2",
	"public/fonts/manrope-latin-wght-normal.woff2",
)
```

此时保留旧 Logo 路由，任务 2 在 HTML 契约切换时一并删除。

- [ ] **步骤 5：运行字体测试验证绿灯**

运行：

```sh
go_docker go test -run '^TestManropeFontIsServedLocally$' -count=1 ./...
```

预期：PASS；响应为 `font/woff2`，文件头为 `wOF2`，许可证存在。

- [ ] **步骤 6：提交字体资产与路由**

```sh
git add public/fonts server.go server_test.go
git diff --cached --check
git commit -m "feat(页面): 本地托管 Manrope 品牌字体"
```

---

### 任务 2：建立单一主操作的语义页面

**文件：**
- 修改：`server_test.go`
- 修改：`server.go`
- 修改：`public/index.html`
- 删除：`public/logo.png`

- [ ] **步骤 1：将旧首页契约改写为新结构红灯测试**

把 `TestStaticAssetsHaveNoRuntimeDependencies` 中与 HTML 和路由有关的断言拆为
`TestLuminousFocusDocumentContract`；保留 `/app.js`、`/styles.css`、`/healthz` 的
Content-Type 检查，使用以下核心断言：

```go
func TestLuminousFocusDocumentContract(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/", nil))
	require.Equal(t, http.StatusOK, response.Code)

	document := response.Body.String()
	lowerDocument := strings.ToLower(document)
	for _, required := range []string{
		`<html lang="zh-CN">`,
		`id="page-title"`,
		`>MyUrls<span aria-hidden="true">.</span></h1>`,
		`把长链接，变得简单。`,
		`id="shorten-form"`,
		`id="long-url"`,
		`id="shorten-button"`,
		`aria-label="生成短链接"`,
		`<details class="custom-key">`,
		`<summary>`,
		`id="short-key"`,
		`id="copy-button"`,
		`id="short-url"`,
		`id="status"`,
		`role="status"`,
		`aria-live="polite"`,
		`href="https://github.com/keleyaa/MyUrls"`,
		`target="_blank"`,
		`rel="noopener noreferrer"`,
		`Go · MIT`,
	} {
		assert.Contains(t, document, required)
	}

	assert.Equal(t, 1, strings.Count(lowerDocument, `<h1`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `<script`))
	assert.Equal(t, 1, strings.Count(lowerDocument, `rel="stylesheet"`))
	assert.Equal(t, 0, strings.Count(lowerDocument, `<img`))
	assert.NotContains(t, lowerDocument, `logo.png`)
	assert.NotContains(t, lowerDocument, `fonts.googleapis.com`)
	assert.NotContains(t, lowerDocument, `fonts.gstatic.com`)
	assert.NotContains(t, lowerDocument, `api.github.com`)

	copyTag := regexp.MustCompile(`<button[^>]+id="copy-button"[^>]*>`).FindString(document)
	require.NotEmpty(t, copyTag)
	assert.Contains(t, copyTag, `hidden`)
	assert.Contains(t, copyTag, `disabled`)

	externalAsset := regexp.MustCompile(`(?:src|href)="https?://`).FindAllString(document, -1)
	assert.Len(t, externalAsset, 1, "only the user-initiated GitHub link may be external")

	for _, route := range router.Routes() {
		assert.NotEqual(t, "/logo.png", route.Path)
	}
}
```

- [ ] **步骤 2：运行页面契约验证红灯**

运行：

```sh
go_docker go test -run '^TestLuminousFocusDocumentContract$' -count=1 ./...
```

预期：FAIL；旧文档仍含 `logo.png`、图片与“创建短链接”标题，且没有 `details` 和页脚。

- [ ] **步骤 3：用新语义结构替换首页**

将 `public/index.html` 重写为以下结构；SVG `path` 直接内联，不引用外部图标：

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="theme-color" content="#f4f7ff" media="(prefers-color-scheme: light)">
  <meta name="theme-color" content="#080b14" media="(prefers-color-scheme: dark)">
  <title>MyUrls</title>
  <link rel="stylesheet" href="/styles.css">
</head>
<body>
  <div class="ambient-light" aria-hidden="true"></div>
  <main class="app-shell">
    <header class="brand">
      <h1 id="page-title" class="wordmark">MyUrls<span aria-hidden="true">.</span></h1>
      <p>把长链接，变得简单。</p>
    </header>

    <section class="tool-surface" aria-labelledby="tool-title">
      <h2 id="tool-title" class="sr-only">创建短链接</h2>
      <form id="shorten-form" action="/short" method="post" aria-busy="false" data-state="idle">
        <div class="url-composer">
          <label class="sr-only" for="long-url">长链接</label>
          <input id="long-url" name="longUrl" type="url" inputmode="url"
            autocomplete="url" placeholder="https://example.com/path" required
            aria-describedby="status">
          <button id="shorten-button" type="submit" aria-label="生成短链接">
            <svg class="submit-arrow" viewBox="0 0 24 24" aria-hidden="true">
              <path d="M5 12h13M13 6l6 6-6 6"/>
            </svg>
            <span class="spinner" aria-hidden="true"></span>
          </button>
        </div>

        <details class="custom-key">
          <summary>自定义短码 <span>可选</span></summary>
          <div class="custom-key-content">
            <label for="short-key">自定义短码</label>
            <input id="short-key" name="shortKey" type="text" autocomplete="off"
              maxlength="64" pattern="[A-Za-z0-9_\-]{1,64}"
              aria-describedby="short-key-help" placeholder="例如 docs_2026">
            <p id="short-key-help">1–64 位，仅限字母、数字、下划线和连字符。</p>
          </div>
        </details>

        <button id="copy-button" class="result-surface" type="button"
          aria-describedby="status" hidden disabled>
          <span class="result-copy">
            <span class="result-label">短链接</span>
            <span id="short-url"></span>
          </span>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect x="8" y="8" width="11" height="11" rx="2"/>
            <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>
          </svg>
        </button>

        <p id="status" role="status" aria-live="polite" aria-atomic="true"
          data-state="idle">粘贴链接后按回车，或点按箭头。</p>
      </form>
    </section>

    <footer class="project-footer">
      <a href="https://github.com/keleyaa/MyUrls" target="_blank"
        rel="noopener noreferrer" aria-label="在 GitHub 打开 keleyaa/MyUrls 仓库">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 .7a11.5 11.5 0 0 0-3.6 22.4c.6.1.8-.3.8-.6v-2.2c-3.3.7-4-1.4-4-1.4-.5-1.4-1.3-1.7-1.3-1.7-1.1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.7 0-1.3.5-2.3 1.2-3.1-.1-.3-.5-1.6.1-3.1 0 0 1-.3 3.2 1.2a11 11 0 0 1 5.8 0c2.2-1.5 3.2-1.2 3.2-1.2.6 1.5.2 2.8.1 3.1.8.8 1.2 1.8 1.2 3.1 0 4.4-2.8 5.4-5.5 5.7.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A11.5 11.5 0 0 0 12 .7Z"/>
        </svg>
        <span><strong>keleyaa/MyUrls</strong><small>Go · MIT</small></span>
        <span class="external-arrow" aria-hidden="true">↗</span>
      </a>
    </footer>
  </main>
  <script src="/app.js" defer></script>
</body>
</html>
```

- [ ] **步骤 4：移除旧 Logo 文件与路由**

从 `server.go` 删除：

```go
router.StaticFile("/logo.png", "public/logo.png")
```

运行：

```sh
rm public/logo.png
```

- [ ] **步骤 5：运行文档契约验证绿灯**

运行：

```sh
go_docker go test -run '^(TestLuminousFocusDocumentContract|TestManropeFontIsServedLocally)$' -count=1 ./...
```

预期：PASS；页面只有文字品牌、一个默认主操作、折叠短码、隐藏结果按钮与底部仓库链接。

- [ ] **步骤 6：提交语义页面**

```sh
git add server.go server_test.go public/index.html public/logo.png
git diff --cached --check
git commit -m "feat(页面): 重建单一主操作语义结构"
```

---


### 任务 3：实现 Luminous Focus 自适应视觉系统

**文件：**
- 修改：`server_test.go`
- 修改：`public/styles.css`

- [ ] **步骤 1：编写 CSS 视觉与无障碍契约红灯测试**

在 `server_test.go` 添加：

```go
func TestLuminousFocusStylesContract(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/styles.css", nil))
	require.Equal(t, http.StatusOK, response.Code)

	styles := strings.ToLower(response.Body.String())
	for _, required := range []string{
		"@font-face",
		"/fonts/manrope-latin-wght-normal.woff2",
		"font-display: swap",
		"font-weight: 200 800",
		"color-scheme: light dark",
		"radial-gradient",
		"backdrop-filter",
		"prefers-color-scheme: dark",
		"prefers-reduced-motion: reduce",
		"prefers-reduced-transparency: reduce",
		"prefers-contrast: more",
		":focus-visible",
		"min-width: 20rem",
		"[hidden]",
	} {
		assert.Contains(t, styles, required)
	}
	for _, forbidden := range []string{
		"@import",
		"url(http",
		"fonts.googleapis.com",
		"fonts.gstatic.com",
	} {
		assert.NotContains(t, styles, forbidden)
	}
}
```

- [ ] **步骤 2：运行 CSS 契约验证红灯**

运行：

```sh
go_docker go test -run '^TestLuminousFocusStylesContract$' -count=1 ./...
```

预期：FAIL；旧 CSS 没有本地字体、深色主题、渐变材质和三个无障碍媒体查询。

- [ ] **步骤 3：建立字体、主题 token 与全局基础**

用以下代码作为 `public/styles.css` 的开头：

```css
@font-face {
  font-family: "Manrope";
  src: url("/fonts/manrope-latin-wght-normal.woff2") format("woff2");
  font-style: normal;
  font-weight: 200 800;
  font-display: swap;
}

:root {
  color-scheme: light dark;
  --font-body: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --font-brand: "Manrope", var(--font-body);
  --page: #f4f7ff;
  --ink: #12192a;
  --muted: #657087;
  --accent: #1769ff;
  --accent-strong: #0757df;
  --surface: rgba(255, 255, 255, 0.7);
  --surface-solid: rgba(255, 255, 255, 0.93);
  --surface-border: rgba(255, 255, 255, 0.9);
  --field: rgba(246, 248, 253, 0.95);
  --line: rgba(74, 91, 126, 0.16);
  --success: #177245;
  --error: #b3262e;
  --focus: #1769ff;
  --shadow: 0 28px 80px rgba(45, 69, 121, 0.16), 0 6px 22px rgba(45, 69, 121, 0.08);
}

* {
  box-sizing: border-box;
}

[hidden] {
  display: none !important;
}

html {
  min-width: 20rem;
  min-height: 100%;
  background: var(--page);
}

body {
  min-width: 20rem;
  min-height: 100vh;
  min-height: 100svh;
  margin: 0;
  overflow-x: hidden;
  color: var(--ink);
  background: var(--page);
  font-family: var(--font-body);
  -webkit-font-smoothing: antialiased;
}

button,
input,
summary {
  font: inherit;
}

button,
summary {
  -webkit-tap-highlight-color: transparent;
}

.ambient-light {
  position: fixed;
  inset: 0;
  z-index: -1;
  pointer-events: none;
  background:
    radial-gradient(circle at 18% 16%, rgba(82, 147, 255, 0.24), transparent 36rem),
    radial-gradient(circle at 82% 22%, rgba(157, 120, 255, 0.18), transparent 32rem),
    radial-gradient(circle at 52% 96%, rgba(109, 211, 255, 0.12), transparent 30rem);
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
```

- [ ] **步骤 4：实现布局、材质、控件和状态**

在同一文件继续添加以下完整组件规则：

```css
.app-shell {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: min(36.875rem, calc(100% - 2rem));
  min-height: 100vh;
  min-height: 100svh;
  margin: 0 auto;
  padding: clamp(3rem, 8vh, 6.5rem) 0 2rem;
}

.brand {
  margin-bottom: 2rem;
  text-align: center;
}

.wordmark {
  margin: 0;
  font-family: var(--font-brand);
  font-size: clamp(3.2rem, 11vw, 5.4rem);
  font-optical-sizing: auto;
  font-weight: 800;
  letter-spacing: -0.07em;
  line-height: 0.96;
}

.wordmark span {
  color: var(--accent);
}

.brand p {
  margin: 1rem 0 0;
  color: var(--muted);
  font-size: clamp(1rem, 2.5vw, 1.125rem);
  letter-spacing: 0.02em;
}

.tool-surface {
  width: 100%;
  padding: clamp(1rem, 3vw, 1.35rem);
  border: 1px solid var(--surface-border);
  border-radius: 2rem;
  background: var(--surface);
  box-shadow: var(--shadow);
  backdrop-filter: blur(28px) saturate(150%);
  -webkit-backdrop-filter: blur(28px) saturate(150%);
}

#shorten-form {
  display: grid;
  gap: 0.875rem;
}

.url-composer {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 3.125rem;
  gap: 0.5rem;
  padding: 0.375rem;
  border: 1px solid var(--line);
  border-radius: 1.35rem;
  background: var(--field);
  transition: border-color 180ms ease, box-shadow 180ms ease, transform 180ms ease;
}

.url-composer:focus-within {
  border-color: color-mix(in srgb, var(--accent) 58%, transparent);
  box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 13%, transparent);
}

#long-url,
#short-key {
  width: 100%;
  min-width: 0;
  border: 0;
  outline: 0;
  color: var(--ink);
  background: transparent;
}

#long-url {
  height: 3.125rem;
  padding: 0 0.8rem;
  font-size: 1rem;
}

input::placeholder {
  color: color-mix(in srgb, var(--muted) 76%, transparent);
}

#shorten-button {
  display: grid;
  width: 3.125rem;
  height: 3.125rem;
  padding: 0;
  place-items: center;
  border: 0;
  border-radius: 1rem;
  color: #fff;
  background: var(--accent);
  box-shadow: 0 9px 22px color-mix(in srgb, var(--accent) 28%, transparent);
  cursor: pointer;
  transition: transform 110ms ease, background-color 180ms ease, box-shadow 180ms ease;
}

#shorten-button:hover:not(:disabled) {
  background: var(--accent-strong);
  box-shadow: 0 11px 26px color-mix(in srgb, var(--accent) 34%, transparent);
}

#shorten-button:active:not(:disabled) {
  transform: scale(0.94);
}

#shorten-button:disabled {
  cursor: progress;
  opacity: 0.72;
}

.submit-arrow {
  width: 1.35rem;
  fill: none;
  stroke: currentColor;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.spinner {
  display: none;
  width: 1.15rem;
  height: 1.15rem;
  border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 760ms linear infinite;
}

#shorten-form[data-state="loading"] .submit-arrow {
  display: none;
}

#shorten-form[data-state="loading"] .spinner {
  display: block;
}

.custom-key {
  padding: 0 0.25rem;
}

.custom-key summary {
  width: fit-content;
  padding: 0.45rem 0.3rem;
  color: var(--muted);
  font-size: 0.875rem;
  cursor: pointer;
  list-style: none;
  transition: color 180ms ease;
}

.custom-key summary::-webkit-details-marker {
  display: none;
}

.custom-key summary::after {
  content: "+";
  display: inline-block;
  margin-left: 0.4rem;
  color: var(--accent);
  transition: transform 180ms ease;
}

.custom-key[open] summary::after {
  transform: rotate(45deg);
}

.custom-key summary span {
  opacity: 0.7;
}

.custom-key-content {
  display: grid;
  gap: 0.45rem;
  padding: 0.5rem 0.25rem 0.15rem;
}

.custom-key-content label {
  color: var(--ink);
  font-size: 0.875rem;
  font-weight: 650;
}

#short-key {
  height: 2.9rem;
  padding: 0 0.85rem;
  border: 1px solid var(--line);
  border-radius: 0.9rem;
  background: var(--surface-solid);
}

#short-key-help {
  margin: 0;
  color: var(--muted);
  font-size: 0.78rem;
  line-height: 1.5;
}

.result-surface {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 1.35rem;
  gap: 0.75rem;
  width: 100%;
  min-height: 4.35rem;
  padding: 0.8rem 1rem;
  align-items: center;
  border: 1px solid color-mix(in srgb, var(--accent) 20%, var(--line));
  border-radius: 1.2rem;
  color: var(--ink);
  background: color-mix(in srgb, var(--accent) 7%, var(--surface-solid));
  cursor: pointer;
  text-align: left;
  animation: result-enter 300ms cubic-bezier(0.22, 0.8, 0.25, 1) both;
  transition: transform 110ms ease, border-color 180ms ease, background-color 180ms ease;
}

.result-surface:hover {
  border-color: color-mix(in srgb, var(--accent) 42%, var(--line));
}

.result-surface:active {
  transform: scale(0.985);
}

.result-copy {
  display: grid;
  min-width: 0;
  gap: 0.15rem;
}

.result-label {
  color: var(--muted);
  font-size: 0.75rem;
  font-weight: 650;
}

#short-url {
  overflow: hidden;
  color: var(--accent-strong);
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.result-surface svg {
  width: 1.25rem;
  fill: none;
  stroke: var(--accent);
  stroke-width: 1.8;
}

#status {
  min-height: 1.4rem;
  margin: 0;
  padding: 0 0.3rem;
  color: var(--muted);
  font-size: 0.82rem;
  line-height: 1.5;
}

#status[data-state="success"] {
  color: var(--success);
}

#status[data-state="invalid"],
#status[data-state="request-error"],
#status[data-state="copy-error"] {
  color: var(--error);
}

.project-footer {
  width: 100%;
  margin-top: clamp(2.1rem, 6vh, 3.75rem);
}

.project-footer a {
  display: grid;
  grid-template-columns: 1.45rem minmax(0, 1fr) auto;
  gap: 0.75rem;
  width: fit-content;
  max-width: 100%;
  margin: 0 auto;
  padding: 0.7rem 0.8rem;
  align-items: center;
  border-radius: 1rem;
  color: var(--muted);
  text-decoration: none;
  transition: color 180ms ease, background-color 180ms ease, transform 110ms ease;
}

.project-footer a:hover {
  color: var(--ink);
  background: color-mix(in srgb, var(--surface-solid) 64%, transparent);
}

.project-footer a:active {
  transform: scale(0.98);
}

.project-footer svg {
  width: 1.4rem;
  fill: currentColor;
}

.project-footer strong,
.project-footer small {
  display: block;
}

.project-footer strong {
  font-size: 0.875rem;
}

.project-footer small {
  margin-top: 0.1rem;
  font-size: 0.72rem;
  opacity: 0.72;
}

.external-arrow {
  font-size: 0.9rem;
}

:focus-visible {
  outline: 3px solid var(--focus);
  outline-offset: 3px;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

@keyframes result-enter {
  from { opacity: 0; transform: translateY(0.45rem) scale(0.985); filter: blur(5px); }
  to { opacity: 1; transform: translateY(0) scale(1); filter: blur(0); }
}
```

- [ ] **步骤 5：增加明暗、移动端与三种无障碍媒体查询**

在 `public/styles.css` 末尾添加：

```css
@media (prefers-color-scheme: dark) {
  :root {
    --page: #080b14;
    --ink: #f2f5ff;
    --muted: #99a4ba;
    --accent: #6f9fff;
    --accent-strong: #92b5ff;
    --surface: rgba(20, 27, 43, 0.7);
    --surface-solid: rgba(23, 31, 49, 0.95);
    --surface-border: rgba(168, 187, 226, 0.13);
    --field: rgba(15, 21, 34, 0.88);
    --line: rgba(168, 187, 226, 0.16);
    --success: #68d59b;
    --error: #ff8b91;
    --focus: #8db4ff;
    --shadow: 0 30px 90px rgba(0, 0, 0, 0.42), 0 6px 24px rgba(0, 0, 0, 0.24);
  }

  .ambient-light {
    background:
      radial-gradient(circle at 18% 16%, rgba(40, 101, 211, 0.25), transparent 34rem),
      radial-gradient(circle at 82% 18%, rgba(102, 67, 180, 0.2), transparent 31rem),
      radial-gradient(circle at 52% 96%, rgba(38, 131, 166, 0.13), transparent 29rem);
  }
}

@media (max-width: 32rem) {
  .app-shell {
    width: min(100% - 2rem, 36.875rem);
    justify-content: flex-start;
    padding-top: clamp(3.5rem, 12vh, 6rem);
  }

  .brand {
    margin-bottom: 1.65rem;
  }

  .tool-surface {
    border-radius: 1.6rem;
  }

  .url-composer {
    grid-template-columns: minmax(0, 1fr) 2.75rem;
  }

  #long-url,
  #shorten-button {
    height: 2.75rem;
  }

  #shorten-button {
    width: 2.75rem;
    border-radius: 0.85rem;
  }
}

@media (prefers-reduced-motion: reduce) {
  html:focus-within {
    scroll-behavior: auto;
  }

  *,
  *::before,
  *::after {
    transition-duration: 0.01ms !important;
    transition-delay: 0ms !important;
  }

  .result-surface {
    animation: none;
  }
}

@media (prefers-reduced-transparency: reduce) {
  .tool-surface {
    background: var(--surface-solid);
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
  }
}

@media (prefers-contrast: more) {
  :root {
    --surface: var(--surface-solid);
    --line: currentColor;
  }

  .tool-surface,
  .url-composer,
  #short-key,
  .result-surface {
    border-width: 2px;
  }

  :focus-visible {
    outline-width: 4px;
  }
}
```

- [ ] **步骤 6：运行 CSS 与页面契约验证绿灯**

运行：

```sh
go_docker go test -run '^(TestLuminousFocusStylesContract|TestLuminousFocusDocumentContract|TestManropeFontIsServedLocally)$' -count=1 ./...
```

预期：PASS；CSS 只引用本地字体，并同时包含自动深色、移动端、减少动态、减少透明度和高对比度分支。

- [ ] **步骤 7：提交视觉系统**

```sh
git add public/styles.css server_test.go
git diff --cached --check
git commit -m "feat(页面): 实现 Luminous Focus 自适应视觉"
```

---

### 任务 4：用浏览器测试驱动六态交互与复制恢复

**文件：**
- 修改：`tests/e2e/app.spec.js`
- 修改：`public/app.js`
- 修改：`server_test.go`

- [ ] **步骤 1：先编写确定性的浏览器行为测试**

用以下内容替换 `tests/e2e/app.spec.js`。所有 `/short` 请求由浏览器测试拦截，
因此业务状态不会依赖 Redis 中已有的短码：

```js
const { expect, test } = require('@playwright/test')

function deferred() {
  let resolve
  const promise = new Promise((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

test('keeps the default path to one action and validates before requesting', async ({ page }) => {
  let requestCount = 0
  await page.route('**/short', async (route) => {
    requestCount += 1
    await route.fulfill({ json: { Code: 1, ShortUrl: 'https://sho.rt/not-used' } })
  })

  await page.goto('/')
  await expect(page).toHaveTitle('MyUrls')
  await expect(page.getByRole('heading', { name: 'MyUrls.' })).toBeVisible()
  await expect(page.getByText('把长链接，变得简单。')).toBeVisible()
  await expect(page.getByLabel('长链接')).toBeVisible()
  await expect(page.getByLabel('自定义短码')).toBeHidden()
  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#status')).toHaveText('粘贴链接后按回车，或点按箭头。')

  await page.getByLabel('长链接').fill('ftp://example.com/file')
  await page.getByLabel('长链接').press('Enter')
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'invalid')
  await expect(page.locator('#status')).toHaveText('请输入以 http:// 或 https:// 开头的有效链接。')
  await expect(page.getByLabel('长链接')).toBeFocused()
  expect(requestCount).toBe(0)

  const repository = page.getByRole('link', { name: '在 GitHub 打开 keleyaa/MyUrls 仓库' })
  await expect(repository).toHaveAttribute('href', 'https://github.com/keleyaa/MyUrls')
  await expect(repository).toHaveAttribute('rel', 'noopener noreferrer')
})

test('submits with Enter, exposes loading, auto-copies, and copies again', async ({ page }) => {
  const requestObserved = deferred()
  const releaseRequest = deferred()
  const expectedShortURL = 'https://sho.rt/luminous'

  await page.route('**/short', async (route) => {
    requestObserved.resolve()
    await releaseRequest.promise
    await route.fulfill({ json: { Code: 1, ShortUrl: expectedShortURL } })
  })

  await page.goto('/')
  await page.getByText('自定义短码', { exact: false }).first().click()
  await page.getByLabel('自定义短码').fill('luminous')
  await page.getByLabel('长链接').fill('https://example.com/long/path')
  await page.getByLabel('长链接').press('Enter')
  await requestObserved.promise

  const submit = page.locator('#shorten-button')
  await expect(submit).toBeDisabled()
  await expect(page.locator('#shorten-form')).toHaveAttribute('aria-busy', 'true')
  await expect(page.locator('#shorten-form')).toHaveAttribute('data-state', 'loading')
  await expect(page.locator('#status')).toHaveText('正在生成短链接…')

  releaseRequest.resolve()
  await expect(page.locator('#short-url')).toHaveText(expectedShortURL)
  await expect(page.locator('#copy-button')).toBeVisible()
  await expect(page.locator('#copy-button')).toBeEnabled()
  await expect(page.locator('#status')).toHaveText('已生成并自动复制。')
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(expectedShortURL)

  await page.locator('#copy-button').click()
  await expect(page.locator('#status')).toHaveText('短链接已复制。')
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(expectedShortURL)
})

test('keeps backend details private and clears stale results', async ({ page }) => {
  let attempt = 0
  await page.route('**/short', async (route) => {
    attempt += 1
    if (attempt === 1) {
      await route.fulfill({ json: { Code: 1, ShortUrl: 'https://sho.rt/first' } })
      return
    }
    await route.fulfill({
      status: 200,
      json: { Code: 1001, Message: 'internal redis key already exists' },
    })
  })

  await page.goto('/')
  await page.getByLabel('长链接').fill('https://example.com/first')
  await page.getByLabel('长链接').press('Enter')
  await expect(page.locator('#copy-button')).toBeVisible()

  await page.getByLabel('长链接').fill('https://example.com/second')
  await page.getByLabel('长链接').press('Enter')
  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'request-error')
  await expect(page.locator('#status')).toHaveText('短链接生成失败，请稍后重试。')
  await expect(page.locator('#status')).not.toContainText('redis')
})

test('uses the textarea fallback and preserves the result when both copy paths fail', async ({ page }) => {
  await page.addInitScript(() => {
    window.__copyShouldSucceed = false
    Object.defineProperty(Navigator.prototype, 'clipboard', {
      configurable: true,
      get: () => undefined,
    })
    Object.defineProperty(Document.prototype, 'execCommand', {
      configurable: true,
      value(command) {
        const target = this.activeElement
        window.__copyFallback = {
          command,
          tagName: target?.tagName,
          value: target?.value,
          readOnly: target?.readOnly,
        }
        return command === 'copy' && window.__copyShouldSucceed
      },
    })
  })
  await page.route('**/short', async (route) => {
    await route.fulfill({ json: { Code: 1, ShortUrl: 'https://sho.rt/fallback' } })
  })

  await page.goto('/')
  await page.getByLabel('长链接').fill('https://example.com/fallback')
  await page.getByLabel('长链接').press('Enter')

  await expect(page.locator('#status')).toHaveAttribute('data-state', 'copy-error')
  await expect(page.locator('#status')).toHaveText('已生成，请手动复制。')
  await expect(page.locator('#copy-button')).toBeVisible()
  await expect.poll(() => page.evaluate(() => window.__copyFallback)).toEqual({
    command: 'copy',
    tagName: 'TEXTAREA',
    value: 'https://sho.rt/fallback',
    readOnly: true,
  })
  await expect(page.locator('textarea')).toHaveCount(0)

  await page.evaluate(() => { window.__copyShouldSucceed = true })
  await page.locator('#copy-button').click()
  await expect(page.locator('#status')).toHaveText('短链接已复制。')
})
```

- [ ] **步骤 2：重建本地服务并观察浏览器红灯**

运行：

```sh
mkdir -p build/local-preview/redis
MYURLS_PORT=18080 MYURLS_DOMAIN=127.0.0.1:18080 MYURLS_PROTO=http MYURLS_REDIS_DATA_PATH=./build/local-preview/redis docker compose -p myurls-preview up -d --build
PLAYWRIGHT_BASE_URL=http://127.0.0.1:18080 npx playwright test tests/e2e/app.spec.js
```

预期：FAIL；旧 `app.js` 把 `#short-url` 当输入框，不会向新结果 `<span>` 写入文本，
也不会使用临时 textarea fallback 或六种 `data-state`。

- [ ] **步骤 3：增加 JavaScript 静态契约**

在 `server_test.go` 添加：

```go
func TestLuminousFocusClientScriptContract(t *testing.T) {
	router := NewRouter(defaultConfig(), Dependencies{
		Ping: func(context.Context) error { return nil },
	})
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequest(http.MethodGet, "/app.js", nil))
	require.Equal(t, http.StatusOK, response.Code)

	script := strings.ToLower(response.Body.String())
	for _, required := range []string{
		"new formdata()",
		"fetch('/short'",
		"navigator.clipboard",
		"document.createtextnode",
		"document.createelement('textarea')",
		"document.execcommand('copy')",
		"request-error",
		"copy-error",
		"aria-busy",
	} {
		assert.Contains(t, script, required)
	}
	for _, forbidden := range []string{
		"btoa(", "unpkg.com", "jsdelivr.net", "api.github.com",
	} {
		assert.NotContains(t, script, forbidden)
	}
}
```

这里的 `document.createTextNode` 用于安全写入结果文本；不能用 `innerHTML`。

- [ ] **步骤 4：用六态实现替换 `public/app.js`**

将文件完整替换为：

```js
'use strict'

const messages = Object.freeze({
  invalidURL: '请输入以 http:// 或 https:// 开头的有效链接。',
  invalidKey: '自定义短码只能使用 1–64 位字母、数字、下划线或连字符。',
  loading: '正在生成短链接…',
  requestError: '短链接生成失败，请稍后重试。',
  copiedAutomatically: '已生成并自动复制。',
  copyAfterCreateFailed: '已生成，请手动复制。',
  copiedAgain: '短链接已复制。',
  copyAgainFailed: '复制失败，请手动选择并复制。',
})

async function createShortURL(longUrl, shortKey) {
  const data = new FormData()
  data.append('longUrl', longUrl)
  data.append('shortKey', shortKey)

  const response = await fetch('/short', { method: 'POST', body: data })
  if (!response.ok) {
    throw new Error('request failed')
  }

  const payload = await response.json()
  if (payload?.Code !== 1 || typeof payload.ShortUrl !== 'string' || payload.ShortUrl === '') {
    throw new Error('request failed')
  }
  return payload.ShortUrl
}

function copyWithTemporaryTextarea(value) {
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.readOnly = true
  textarea.tabIndex = -1
  textarea.setAttribute('aria-hidden', 'true')
  textarea.style.position = 'fixed'
  textarea.style.inset = '0 auto auto -9999px'
  document.body.append(textarea)

  try {
    textarea.focus()
    textarea.select()
    textarea.setSelectionRange(0, textarea.value.length)
    if (!document.execCommand('copy')) {
      throw new Error('copy failed')
    }
  } finally {
    textarea.remove()
  }
}

async function copyText(value) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value)
      return
    } catch {
      // Continue to the local textarea fallback.
    }
  }
  copyWithTemporaryTextarea(value)
}

function isValidHTTPURL(value) {
  if (!value || value !== value.trim()) {
    return false
  }
  try {
    const parsed = new URL(value)
    return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== ''
  } catch {
    return false
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const form = document.querySelector('#shorten-form')
  const longURLInput = document.querySelector('#long-url')
  const shortKeyInput = document.querySelector('#short-key')
  const shortURL = document.querySelector('#short-url')
  const shortenButton = document.querySelector('#shorten-button')
  const copyButton = document.querySelector('#copy-button')
  const status = document.querySelector('#status')

  function setStatus(state, message) {
    form.dataset.state = state
    status.dataset.state = state
    status.textContent = message
  }

  function setBusy(isBusy) {
    shortenButton.disabled = isBusy
    shortenButton.setAttribute('aria-label', isBusy ? '正在生成短链接' : '生成短链接')
    form.setAttribute('aria-busy', String(isBusy))
  }

  function clearResult() {
    shortURL.replaceChildren()
    copyButton.hidden = true
    copyButton.disabled = true
    copyButton.removeAttribute('aria-label')
    copyButton.removeAttribute('title')
  }

  function showResult(value) {
    shortURL.replaceChildren(document.createTextNode(value))
    copyButton.hidden = false
    copyButton.disabled = false
    copyButton.setAttribute('aria-label', `复制短链接 ${value}`)
    copyButton.title = value
  }

  form.addEventListener('submit', async (event) => {
    event.preventDefault()

    const longUrl = longURLInput.value
    if (!isValidHTTPURL(longUrl)) {
      clearResult()
      setStatus('invalid', messages.invalidURL)
      longURLInput.focus()
      return
    }
    if (!shortKeyInput.checkValidity()) {
      clearResult()
      setStatus('invalid', messages.invalidKey)
      shortKeyInput.focus()
      shortKeyInput.reportValidity()
      return
    }

    clearResult()
    setBusy(true)
    setStatus('loading', messages.loading)

    try {
      const value = await createShortURL(longUrl, shortKeyInput.value)
      let copied = true
      try {
        await copyText(value)
      } catch {
        copied = false
      }
      showResult(value)
      setStatus(
        copied ? 'success' : 'copy-error',
        copied ? messages.copiedAutomatically : messages.copyAfterCreateFailed,
      )
    } catch {
      clearResult()
      setStatus('request-error', messages.requestError)
    } finally {
      setBusy(false)
    }
  })

  copyButton.addEventListener('click', async () => {
    const value = shortURL.textContent
    if (!value) {
      clearResult()
      return
    }
    try {
      await copyText(value)
      setStatus('success', messages.copiedAgain)
    } catch {
      setStatus('copy-error', messages.copyAgainFailed)
    }
  })
})
```

- [ ] **步骤 5：运行静态契约和浏览器测试验证绿灯**

运行：

```sh
go_docker go test -run '^(TestLuminousFocusClientScriptContract|TestLuminousFocusDocumentContract)$' -count=1 ./...
MYURLS_PORT=18080 MYURLS_DOMAIN=127.0.0.1:18080 MYURLS_PROTO=http MYURLS_REDIS_DATA_PATH=./build/local-preview/redis docker compose -p myurls-preview up -d --build
PLAYWRIGHT_BASE_URL=http://127.0.0.1:18080 npx playwright test tests/e2e/app.spec.js
```

预期：Go 契约 PASS；当前两个 Playwright 项目共 8 个用例 PASS。自动复制、再次复制、
业务错误脱敏和 textarea fallback 均有真实浏览器证据。

- [ ] **步骤 6：提交交互实现**

```sh
git add public/app.js server_test.go tests/e2e/app.spec.js
git diff --cached --check
git commit -m "feat(页面): 实现短链六态交互与复制恢复"
```

---

### 任务 5：扩展为四种主题/视口项目并保存成功态截图

**文件：**
- 修改：`playwright.config.js`
- 修改：`tests/e2e/app.spec.js`

- [ ] **步骤 1：把 Playwright 固定为四个批准的视觉项目**

用以下内容替换 `playwright.config.js`：

```js
const { defineConfig } = require('@playwright/test')

module.exports = defineConfig({
  testDir: './tests/e2e',
  timeout: 30_000,
  expect: { timeout: 5_000 },
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: [
    ['list'],
    ['html', { open: 'never' }],
  ],
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL || 'http://127.0.0.1:8080',
    permissions: ['clipboard-read', 'clipboard-write'],
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'Desktop Light',
      use: {
        browserName: 'chromium',
        viewport: { width: 1440, height: 900 },
        colorScheme: 'light',
      },
    },
    {
      name: 'Desktop Dark',
      use: {
        browserName: 'chromium',
        viewport: { width: 1440, height: 900 },
        colorScheme: 'dark',
      },
    },
    {
      name: 'Mobile Light',
      use: {
        browserName: 'chromium',
        viewport: { width: 390, height: 844 },
        colorScheme: 'light',
        hasTouch: true,
        isMobile: true,
      },
    },
    {
      name: 'Mobile Dark',
      use: {
        browserName: 'chromium',
        viewport: { width: 390, height: 844 },
        colorScheme: 'dark',
        hasTouch: true,
        isMobile: true,
      },
    },
  ],
})
```

- [ ] **步骤 2：增加主题、边界、重叠和截图验收**

在 `tests/e2e/app.spec.js` 增加以下辅助函数：

```js
function projectSlug(projectName) {
  return projectName.toLowerCase().replace(/[^a-z0-9]+/g, '-')
}
```

把成功工作流测试的签名改为：

```js
test('submits with Enter, exposes loading, auto-copies, and copies again', async ({ page }, testInfo) => {
```

并在该测试最后增加：

```js
await page.screenshot({
  path: testInfo.outputPath(`${projectSlug(testInfo.project.name)}-success.png`),
  fullPage: true,
})
```

再添加第五个测试：

```js
test('matches the project theme and keeps visible controls inside the viewport', async ({ page }, testInfo) => {
  const browserErrors = []
  page.on('console', (message) => {
    if (message.type() === 'error') {
      browserErrors.push(`console: ${message.text()}`)
    }
  })
  page.on('pageerror', (error) => browserErrors.push(`pageerror: ${error.message}`))

  await page.goto('/')
  await page.getByText('自定义短码', { exact: false }).first().click()

  const expectedDark = testInfo.project.name.endsWith('Dark')
  const prefersDark = await page.evaluate(() => matchMedia('(prefers-color-scheme: dark)').matches)
  expect(prefersDark).toBe(expectedDark)

  const layout = await page.evaluate(() => {
    const visible = [...document.querySelectorAll('a, button, input, summary')]
      .filter((element) => {
        const style = getComputedStyle(element)
        const rect = element.getBoundingClientRect()
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0
      })
      .map((element) => {
        const rect = element.getBoundingClientRect()
        return {
          name: element.id || element.tagName.toLowerCase(),
          left: rect.left,
          top: rect.top,
          right: rect.right,
          bottom: rect.bottom,
        }
      })

    const outOfBounds = visible.filter((rect) => (
      rect.left < 0 || rect.top < 0 || rect.right > innerWidth || rect.bottom > document.documentElement.scrollHeight
    ))
    const overlaps = []
    for (let left = 0; left < visible.length; left += 1) {
      for (let right = left + 1; right < visible.length; right += 1) {
        const a = visible[left]
        const b = visible[right]
        const intersects = a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
        if (intersects) {
          overlaps.push(`${a.name}:${b.name}`)
        }
      }
    }
    return {
      horizontalOverflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
      outOfBounds,
      overlaps,
    }
  })

  expect(layout.horizontalOverflow).toBe(false)
  expect(layout.outOfBounds).toEqual([])
  expect(layout.overlaps).toEqual([])
  expect(browserErrors).toEqual([])
})
```

- [ ] **步骤 3：列出项目与用例数量**

运行：

```sh
PLAYWRIGHT_BASE_URL=http://127.0.0.1:18080 npx playwright test --list
```

预期：列出 `Desktop Light`、`Desktop Dark`、`Mobile Light`、`Mobile Dark`，
每个项目 5 个测试，总计 20 个测试。

- [ ] **步骤 4：运行四项目浏览器门禁**

运行：

```sh
MYURLS_PORT=18080 MYURLS_DOMAIN=127.0.0.1:18080 MYURLS_PROTO=http MYURLS_REDIS_DATA_PATH=./build/local-preview/redis docker compose -p myurls-preview up -d --build
PLAYWRIGHT_BASE_URL=http://127.0.0.1:18080 npx playwright test
```

预期：20 个测试全部 PASS；每个项目的成功测试目录中各有一张原始分辨率截图。

- [ ] **步骤 5：提交跨主题浏览器覆盖**

```sh
git add playwright.config.js tests/e2e/app.spec.js
git diff --cached --check
git commit -m "test(页面): 覆盖四种主题与响应式视口"
```

---

### 任务 6：完整验证、视觉复核与本地交付

**文件：**
- 验证：`public/index.html`
- 验证：`public/styles.css`
- 验证：`public/app.js`
- 验证：`public/fonts/manrope-latin-wght-normal.woff2`
- 验证：`public/fonts/OFL.txt`
- 验证：`server.go`
- 验证：`server_test.go`
- 验证：`playwright.config.js`
- 验证：`tests/e2e/app.spec.js`

- [ ] **步骤 1：运行格式、差异和外部运行时资源检查**

运行：

```sh
gofmt -w server.go server_test.go
git diff --check
rg -n '(src|href)="https?://' public
rg -n 'api\.github\.com|fonts\.googleapis\.com|unpkg\.com|jsdelivr\.net' public
```

预期：`git diff --check` 无输出；第一条 `rg` 只命中
`public/index.html` 的 `https://github.com/keleyaa/MyUrls`；第二条 `rg` 无匹配并返回 1。

- [ ] **步骤 2：运行完整 Go 门禁**

依次运行：

```sh
go_docker go test -count=1 ./...
go_docker go test -shuffle=on -count=10 ./...
go_docker go test -race -count=1 ./...
go_docker go vet ./...
go_docker go run golang.org/x/vuln/cmd/govulncheck@v1.6.0 ./...
```

预期：测试、shuffle、race、vet 全部退出 0；`govulncheck` 报告
`No vulnerabilities found.`。

- [ ] **步骤 3：确认 npm 依赖仍为当前版本**

运行：

```sh
npm outdated --json
npm test --if-present
```

预期：`npm outdated --json` 输出 `{}`；没有新增运行时 npm 依赖。

- [ ] **步骤 4：重新构建最终镜像并运行浏览器全套**

运行：

```sh
docker build -t myurls:luminous-local .
MYURLS_PORT=18080 MYURLS_DOMAIN=127.0.0.1:18080 MYURLS_PROTO=http MYURLS_REDIS_DATA_PATH=./build/local-preview/redis docker compose -p myurls-preview up -d --build
curl -fsS http://127.0.0.1:18080/healthz
curl -fsSI http://127.0.0.1:18080/fonts/manrope-latin-wght-normal.woff2
PLAYWRIGHT_BASE_URL=http://127.0.0.1:18080 npx playwright test
```

预期：镜像构建成功；健康端点返回成功 JSON；字体响应为 `200` 且 Content-Type
包含 `font/woff2`；20 个浏览器测试全部 PASS。

- [ ] **步骤 5：原始分辨率目视检查四张成功态截图**

使用图片查看工具逐张打开以下四类文件：

```text
test-results/**/desktop-light-success.png
test-results/**/desktop-dark-success.png
test-results/**/mobile-light-success.png
test-results/**/mobile-dark-success.png
```

逐项确认：品牌字光学居中；句点为动作蓝；主材质只有一层；输入和箭头无重叠；
结果地址省略但不挤压复制图标；页脚与主工具留白稳定；深色没有发灰；移动端没有
横向溢出。若发现问题，先增加能复现问题的断言，再修改 CSS 并重跑任务 3、5 门禁。

- [ ] **步骤 6：按 Apple Design 与代码审查清单复核**

调用 `apple-design` 检查动效、材质、字体、焦点和 reduced-motion；调用
`requesting-code-review` 检查规格覆盖、回归与安全边界。审查发现必须修复项时，
回到对应任务补红灯测试、最小修复、全量验证并单独提交。

- [ ] **步骤 7：确认提交链、干净状态与本地预览**

运行：

```sh
git status --short --branch
git log --oneline --decorate -6
curl -fsS http://127.0.0.1:18080/ | rg 'MyUrls|keleyaa/MyUrls'
```

预期：工作区干净；最近提交依次覆盖字体、语义结构、视觉、交互与四项目测试；
本地预览保持运行于 `http://127.0.0.1:18080` 供用户检查。不要在交付时执行
`docker compose down`，也不要推送。
