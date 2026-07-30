# MyUrls dependency version research (2026-07-31)

## Scope and conclusion

This note inventories every versioned dependency found in `go.mod`, `Dockerfile`,
`docker-compose.yaml`, `.github/workflows/*.yml`, and `public/index.html`. Versions
were checked on 2026-07-31 against first-party release repositories and official
package/image registries only.

Recommended fully-current baseline:

- Go module/toolchain: `go 1.25.0` (new dependency floor) plus
  `toolchain go1.26.5` (latest patched compiler).
- Docker builder: `golang:1.26.5-alpine3.24` (or pin its multi-arch digest).
- Redis server: `redis:8.10.0`; `7.4.10` is the newest compatibility-line option.
- Go direct dependencies: miniredis `v2.38.0`, Gin `v1.12.0`, go-redis
  `v9.21.0`, Testify `v1.11.1`, Zap `v1.28.0`, Lumberjack `v2.2.1`.
- GitHub Actions: checkout `v7.0.1`, setup-go `v7.0.0`, upload-artifact
  `v7.0.1`, setup-buildx `v4.2.0`, login `v4.6.0`, build-push `v7.3.0`.
- Existing Vue 2 UI compatibility line: Vue `2.7.16`, Axios `1.19.0`,
  Element UI `2.15.14`, vue-clipboard2 `0.3.3`.

The one dependency that cannot be moved to its latest major in isolation is Vue.
Vue `3.5.40` is the latest stable major, but the page uses the Vue 2 global API
(`new Vue`) and Element UI `2.15.14` declares peer compatibility with Vue 2.
Moving to Vue 3 therefore requires a UI migration (normally to Element Plus), not
just a CDN URL edit. Vue 2 is already end-of-life, so `2.7.16` is only the latest
compatible line, not a maintained long-term target.

## Evidence method

The following commands are reproducible and use the official Go service/module
proxy, Docker Hub API, GitHub repositories, and npm registry:

```sh
# Go stable releases (current and archived)
curl -fsSL 'https://go.dev/dl/?mode=json'
curl -fsSL 'https://go.dev/dl/?mode=json&include=all'

# A direct Go module; repeat for each module path
curl -fsSL 'https://proxy.golang.org/github.com/gin-gonic/gin/@latest'

# Resolve the actual post-upgrade module graph in a disposable copy
go get -u ./...
go mod tidy
# Upgrade modules that only became reachable after the graph rewrite
go get -u all
go mod tidy
go list -m -u -json all

# Official Docker Library tags
curl -fsSL 'https://hub.docker.com/v2/repositories/library/golang/tags/1.26.5-alpine'
curl -fsSL 'https://hub.docker.com/v2/repositories/library/redis/tags/8.10.0'

# Official action repositories (tags are release-owned refs)
git ls-remote --tags --refs https://github.com/actions/checkout.git

# Official npm package metadata
curl -fsSL 'https://registry.npmjs.org/vue/latest'
curl -fsSL 'https://registry.npmjs.org/vue'
```

The two-stage Go upgrade/tidy experiment was run in a temporary copy under the
official `golang:1.26.5-alpine` image, then `go test ./...` passed there. The
second `go get -u all` is intentional: after the direct dependency graph was
rewritten, it advanced newly reachable `gopher-lua` from `v1.1.1` to `v1.1.2`.
The experiment did not modify the working tree. The CDN asset URLs proposed below
all returned HTTP 200.

## Go language and toolchain

| Item | Repository value | Recommended | Latest / compatibility context | Evidence |
| --- | --- | --- | --- | --- |
| `go` directive | `1.24` | `1.25.0` | Go 1.26 is the latest language/toolchain line; `1.25.0` preserves the broadest consumer compatibility while satisfying Gin's new minimum; `1.24.13` is the final 1.24 patch | [Go downloads JSON](https://go.dev/dl/?mode=json), [all releases](https://go.dev/dl/?mode=json&include=all) |
| `toolchain` | `go1.24.3` | `go1.26.5` | Latest stable patch; old-line fallback is `go1.24.13` | [Go release history](https://go.dev/doc/devel/release), [Go 1.26 notes](https://go.dev/doc/go1.26) |

Use `go 1.25.0` to state the minimum language/module baseline required by the
upgraded dependency graph and `toolchain go1.26.5` to select the latest patched
compiler. Raising the `go` directive all the way to 1.26 is valid but needlessly
prevents Go 1.25 consumers from building a graph that Gin itself supports. Gin
`v1.12.0` declares `go 1.25.0`, so the current Go 1.24 baseline cannot be retained
together with all latest direct dependencies. Gin `v1.11.0` (declares Go 1.23) is
the latest Gin compatibility option if the project must temporarily remain on Go
1.24.

Potential breakage: Go 1.26 changes toolchain, runtime, standard-library and
platform behavior. Its official notes include removed toolchain functionality and
platform support changes. Cross-compilation scripts and all target architectures
must therefore be rebuilt and tested, not only the host package tests.

## Direct Go modules

All latest values below come from each module's official Go proxy `@latest`
record. No direct module has a newer stable semantic-import major: attempted
`/v2` or `/v10` lookups for Gin, Testify, Zap and go-redis returned no stable
module. Lumberjack has only `v3.0.0-alpha`, which is deliberately excluded.

| Module | Current | Latest stable / compatible | Evidence and upgrade notes |
| --- | ---: | ---: | --- |
| `github.com/alicebob/miniredis/v2` | `v2.38.0` | `v2.38.0` | [Go proxy record](https://proxy.golang.org/github.com/alicebob/miniredis/v2/@latest); already current |
| `github.com/gin-gonic/gin` | `v1.10.1` | `v1.12.0` | [Go proxy record](https://proxy.golang.org/github.com/gin-gonic/gin/@latest), [changelog](https://github.com/gin-gonic/gin/blob/v1.12.0/CHANGELOG.md); raises minimum Go to 1.25, changes some binding/client-IP behavior, and brings HTTP/3/BSON dependencies |
| `github.com/redis/go-redis/v9` | `v9.8.0` | `v9.21.0` | [Go proxy record](https://proxy.golang.org/github.com/redis/go-redis/v9/@latest), [release](https://github.com/redis/go-redis/releases/tag/v9.21.0); same import major, but Redis command/protocol behavior still needs integration tests |
| `github.com/stretchr/testify` | `v1.10.0` | `v1.11.1` | [Go proxy record](https://proxy.golang.org/github.com/stretchr/testify/@latest), [release](https://github.com/stretchr/testify/releases/tag/v1.11.1); test-only, same major |
| `go.uber.org/zap` | `v1.27.0` | `v1.28.0` | [Go proxy record](https://proxy.golang.org/go.uber.org/zap/@latest), [release](https://github.com/uber-go/zap/releases/tag/v1.28.0); same major |
| `gopkg.in/natefinch/lumberjack.v2` | `v2.2.1` | `v2.2.1` | [Go proxy record](https://proxy.golang.org/gopkg.in/natefinch/lumberjack.v2/@latest); already current; v3 remains alpha and is not stable |

## Indirect Go module graph after upgrade and tidy

Indirect dependencies should not be individually forced to their registry's
highest version. They are selected by Minimal Version Selection from the upgraded
direct dependency graph. The following is the exact `go.mod` indirect block
produced by the two-stage upgrade commands above under Go 1.26.5.

| Resolved indirect module | Version | Change from repository | First-party evidence |
| --- | ---: | --- | --- |
| `github.com/bytedance/gopkg` | `v0.1.4` | added | [module record](https://pkg.go.dev/github.com/bytedance/gopkg@v0.1.4) |
| `github.com/bytedance/sonic` | `v1.15.2` | `v1.13.2` -> | [module record](https://pkg.go.dev/github.com/bytedance/sonic@v1.15.2) |
| `github.com/bytedance/sonic/loader` | `v0.5.2` | `v0.2.4` -> | [module record](https://pkg.go.dev/github.com/bytedance/sonic/loader@v0.5.2) |
| `github.com/cespare/xxhash/v2` | `v2.3.0` | unchanged | [module record](https://pkg.go.dev/github.com/cespare/xxhash/v2@v2.3.0) |
| `github.com/cloudwego/base64x` | `v0.1.7` | `v0.1.5` -> | [module record](https://pkg.go.dev/github.com/cloudwego/base64x@v0.1.7) |
| `github.com/davecgh/go-spew` | `v1.1.1` | unchanged | [module record](https://pkg.go.dev/github.com/davecgh/go-spew@v1.1.1) |
| `github.com/gabriel-vasile/mimetype` | `v1.4.15` | `v1.4.9` -> | [module record](https://pkg.go.dev/github.com/gabriel-vasile/mimetype@v1.4.15) |
| `github.com/gin-contrib/sse` | `v1.1.1` | `v1.1.0` -> | [module record](https://pkg.go.dev/github.com/gin-contrib/sse@v1.1.1) |
| `github.com/go-playground/locales` | `v0.14.1` | unchanged | [module record](https://pkg.go.dev/github.com/go-playground/locales@v0.14.1) |
| `github.com/go-playground/universal-translator` | `v0.18.1` | unchanged | [module record](https://pkg.go.dev/github.com/go-playground/universal-translator@v0.18.1) |
| `github.com/go-playground/validator/v10` | `v10.30.3` | `v10.26.0` -> | [module record](https://pkg.go.dev/github.com/go-playground/validator/v10@v10.30.3) |
| `github.com/goccy/go-json` | `v0.10.6` | `v0.10.5` -> | [module record](https://pkg.go.dev/github.com/goccy/go-json@v0.10.6) |
| `github.com/goccy/go-yaml` | `v1.19.2` | added | [module record](https://pkg.go.dev/github.com/goccy/go-yaml@v1.19.2) |
| `github.com/json-iterator/go` | `v1.1.12` | unchanged | [module record](https://pkg.go.dev/github.com/json-iterator/go@v1.1.12) |
| `github.com/klauspost/cpuid/v2` | `v2.4.0` | `v2.2.10` -> | [module record](https://pkg.go.dev/github.com/klauspost/cpuid/v2@v2.4.0) |
| `github.com/leodido/go-urn` | `v1.5.0` | `v1.4.0` -> | [module record](https://pkg.go.dev/github.com/leodido/go-urn@v1.5.0) |
| `github.com/mattn/go-isatty` | `v0.0.24` | `v0.0.20` -> | [module record](https://pkg.go.dev/github.com/mattn/go-isatty@v0.0.24) |
| `github.com/modern-go/concurrent` | `v0.0.0-20180306012644-bacd9c7ef1dd` | unchanged | [module record](https://pkg.go.dev/github.com/modern-go/concurrent@v0.0.0-20180306012644-bacd9c7ef1dd) |
| `github.com/modern-go/reflect2` | `v1.0.2` | unchanged | [module record](https://pkg.go.dev/github.com/modern-go/reflect2@v1.0.2) |
| `github.com/pelletier/go-toml/v2` | `v2.4.3` | `v2.2.4` -> | [module record](https://pkg.go.dev/github.com/pelletier/go-toml/v2@v2.4.3) |
| `github.com/pmezard/go-difflib` | `v1.0.0` | unchanged | [module record](https://pkg.go.dev/github.com/pmezard/go-difflib@v1.0.0) |
| `github.com/quic-go/qpack` | `v0.6.0` | added by Gin | [module record](https://pkg.go.dev/github.com/quic-go/qpack@v0.6.0) |
| `github.com/quic-go/quic-go` | `v0.61.0` | added by Gin | [module record](https://pkg.go.dev/github.com/quic-go/quic-go@v0.61.0) |
| `github.com/twitchyliquid64/golang-asm` | `v0.15.1` | unchanged | [module record](https://pkg.go.dev/github.com/twitchyliquid64/golang-asm@v0.15.1) |
| `github.com/ugorji/go/codec` | `v1.3.1` | `v1.2.12` -> | [module record](https://pkg.go.dev/github.com/ugorji/go/codec@v1.3.1) |
| `github.com/yuin/gopher-lua` | `v1.1.2` | `v1.1.1` -> after second-stage graph upgrade | [module record](https://pkg.go.dev/github.com/yuin/gopher-lua@v1.1.2) |
| `go.mongodb.org/mongo-driver/v2` | `v2.8.0` | added by Gin BSON support | [module record](https://pkg.go.dev/go.mongodb.org/mongo-driver/v2@v2.8.0) |
| `go.uber.org/atomic` | `v1.11.0` | added by Zap | [module record](https://pkg.go.dev/go.uber.org/atomic@v1.11.0) |
| `go.uber.org/multierr` | `v1.11.0` | unchanged | [module record](https://pkg.go.dev/go.uber.org/multierr@v1.11.0) |
| `golang.org/x/arch` | `v0.29.0` | `v0.17.0` -> | [module record](https://pkg.go.dev/golang.org/x/arch@v0.29.0) |
| `golang.org/x/crypto` | `v0.54.0` | `v0.38.0` -> | [module record](https://pkg.go.dev/golang.org/x/crypto@v0.54.0) |
| `golang.org/x/net` | `v0.57.0` | `v0.40.0` -> | [module record](https://pkg.go.dev/golang.org/x/net@v0.57.0) |
| `golang.org/x/sys` | `v0.47.0` | `v0.33.0` -> | [module record](https://pkg.go.dev/golang.org/x/sys@v0.47.0) |
| `golang.org/x/text` | `v0.40.0` | `v0.25.0` -> | [module record](https://pkg.go.dev/golang.org/x/text@v0.40.0) |
| `google.golang.org/protobuf` | `v1.36.11` | `v1.36.6` -> | [module record](https://pkg.go.dev/google.golang.org/protobuf@v1.36.11) |
| `gopkg.in/yaml.v3` | `v3.0.1` | unchanged | [module record](https://pkg.go.dev/gopkg.in/yaml.v3@v3.0.1) |

`github.com/dgryski/go-rendezvous`, `github.com/kr/pretty`, and
`gopkg.in/check.v1` disappear from the tidy result. New QUIC and MongoDB modules
are upstream implementation dependencies; their presence does not mean MyUrls
itself has enabled HTTP/3 or MongoDB.

## Container images

| Location | Current | Latest stable | Compatible pinned line | Official evidence |
| --- | --- | --- | --- | --- |
| `Dockerfile` builder | `golang:1.24-alpine` (floating Go and Alpine patches) | `golang:1.26.5-alpine3.24` | `golang:1.24.13-alpine3.23` is the final available old-Go line | [1.26.5/Alpine 3.24 tag API](https://hub.docker.com/v2/repositories/library/golang/tags/1.26.5-alpine3.24), [official image manifest](https://github.com/docker-library/official-images/blob/master/library/golang), [official image](https://hub.docker.com/_/golang) |
| Compose Redis | `redis:7` (floating patch) | `redis:8.10.0` | `redis:7.4.10` | [8.10.0 tag API](https://hub.docker.com/v2/repositories/library/redis/tags/8.10.0), [7.4.10 tag API](https://hub.docker.com/v2/repositories/library/redis/tags/7.4.10), [official image](https://hub.docker.com/_/redis) |

Observed multi-arch manifest digests on the research date were
`sha256:0178a641...ff5fb2` for the floating-Alpine
`golang:1.26.5-alpine` tag and
`sha256:c29e49ab...75a5236` for `redis:8.10.0`. Re-query and record full digests at
implementation time if immutable image pinning is desired; tags are mutable.

Potential breakage: Redis 7 -> 8 is a server major upgrade over persistent data.
Redis 8 bundles previously separate data-structure/search capabilities and changes
ACL category behavior for their commands. See the official [Redis 8.0 breaking
changes](https://redis.io/docs/latest/develop/whats-new/8-0/#breaking-changes).
Back up the mounted `./data/redis` directory, validate persistence loading and the
shorten/redirect path, and test rollback constraints before production rollout.

## GitHub Actions

| Action | Current reference | Latest stable tag | Evidence |
| --- | --- | --- | --- |
| `actions/checkout` | `master` | `v7.0.1` | [release](https://github.com/actions/checkout/releases/tag/v7.0.1), [official tags](https://github.com/actions/checkout/tags) |
| `actions/setup-go` | `v5` | `v7.0.0` | [release](https://github.com/actions/setup-go/releases/tag/v7.0.0), [official tags](https://github.com/actions/setup-go/tags) |
| `actions/upload-artifact` | `v4` | `v7.0.1` | [release](https://github.com/actions/upload-artifact/releases/tag/v7.0.1), [official tags](https://github.com/actions/upload-artifact/tags) |
| `docker/setup-buildx-action` | `v3` | `v4.2.0` | [release](https://github.com/docker/setup-buildx-action/releases/tag/v4.2.0), [official tags](https://github.com/docker/setup-buildx-action/tags) |
| `docker/login-action` | `v3` | `v4.6.0` | [release](https://github.com/docker/login-action/releases/tag/v4.6.0), [official tags](https://github.com/docker/login-action/tags) |
| `docker/build-push-action` | `v5` | `v7.3.0` | [release](https://github.com/docker/build-push-action/releases/tag/v7.3.0), [official tags](https://github.com/docker/build-push-action/tags) |

All six latest action releases declare `runs.using: node24` in their official
`action.yml`. GitHub-hosted `ubuntu-latest` is compatible. Self-hosted runners
would need Actions Runner `v2.327.1` or later for Node 24; checkout's credential
storage behavior additionally needs runner `v2.329.0` for authenticated Git from
container actions. See the [checkout v7 README](https://github.com/actions/checkout/blob/v7.0.1/README.md)
and [setup-go v7 README](https://github.com/actions/setup-go/blob/v7.0.0/README.md).

Potential breakage:

- Checkout v7 refuses some fork code checkout under privileged triggers and moves
  persisted credentials into `$RUNNER_TEMP`. The current workflows trigger only on
  pushes, so the fork restriction is not currently active.
- Artifact v4 and newer artifacts are immutable, same-name uploads cannot be
  appended, and hidden files are excluded by default. The three current jobs use
  distinct artifact names and explicit non-hidden archives, so they fit the new
  model. See the official [migration guide](https://github.com/actions/upload-artifact/blob/v7.0.1/docs/MIGRATION.md).
- `actions/checkout@master` is a floating branch, not a stable version. Prefer an
  exact tag for reproducibility, or an immutable full commit SHA for the strongest
  supply-chain pinning. Major tags (for example `@v7`) deliberately float to later
  compatible action releases.

The workflow's `go-version: '^1.24.3'` should be updated with the toolchain. An
exact `1.26.5` is deterministic; `1.26.x` intentionally follows security patches.

## Browser CDN libraries

| Library | Current | Latest stable major | Latest compatible with current page | Evidence |
| --- | ---: | ---: | ---: | --- |
| Vue | `2.6.11` | `3.5.40` | `2.7.16` (`v2-latest`) | [npm metadata](https://registry.npmjs.org/vue), [Vue 2 EOL](https://v2.vuejs.org/eol/), [Vue 3 migration guide](https://v3-migration.vuejs.org/breaking-changes/) |
| Axios | `0.19.2` | `1.19.0` | `1.19.0` for the APIs used here; conservative pre-1 line is `0.33.0` | [latest metadata](https://registry.npmjs.org/axios/latest), [all dist-tags](https://registry.npmjs.org/axios), [changelog](https://github.com/axios/axios/blob/v1.19.0/CHANGELOG.md) |
| Element UI | `2.13.0` | `2.15.14` | `2.15.14` | [latest metadata](https://registry.npmjs.org/element-ui/latest), [release](https://github.com/ElemeFE/element/releases/tag/v2.15.14); peer dependency is Vue `^2.5.17` |
| vue-clipboard2 | `0.3.1` | `0.3.3` | `0.3.3` | [latest metadata](https://registry.npmjs.org/vue-clipboard2/latest), [release](https://github.com/Inndy/vue-clipboard2/releases/tag/0.3.3) |

For the separate Vue 3 migration, the actively maintained Element-family package
is Element Plus `2.14.3`, whose peer dependency requires Vue `^3.3.7`.
[Element Plus npm metadata](https://registry.npmjs.org/element-plus/latest) is the
first-party registry evidence. It is a replacement package, not an in-place
Element UI version upgrade.

Verified compatible-line CDN assets:

```text
https://unpkg.com/vue@2.7.16/dist/vue.min.js
https://unpkg.com/axios@1.19.0/dist/axios.min.js
https://unpkg.com/element-ui@2.15.14/lib/theme-chalk/index.css
https://unpkg.com/element-ui@2.15.14/lib/index.js
https://cdn.jsdelivr.net/npm/vue-clipboard2@0.3.3/dist/vue-clipboard.min.js
```

Potential breakage:

- Vue 3 removes/changes the global construction and mounting API used by this
  inline application, and Element UI 2 does not support Vue 3. Stay on Vue 2.7.16
  for the compatibility release, then migrate Vue and the component library as a
  separate change.
- Axios 0.x -> 1.x is a major change with adapter, header, cancellation, error and
  form-data behavior changes across its history. This page only calls
  `axios.post()` with browser `FormData` and consumes `res.data`, which remains
  supported, but the `/short` request and error path must be checked in a browser.
- CDN URLs should eventually use Subresource Integrity or locally vendored assets.
  Merely pinning a version prevents surprise semver movement but does not provide
  integrity verification.

## Suggested upgrade order

1. Upgrade Go/toolchain and direct modules, run `go mod tidy`, then run
   `go get -u all && go mod tidy` on the rewritten graph; run unit tests, vet,
   race tests where supported, and all target builds.
2. Upgrade Actions and confirm all CI artifacts and the multi-architecture image.
3. Back up Redis data, move to Redis 8.10.0, and verify persistence plus both API
   and redirect behavior.
4. Upgrade the current UI compatibility line (Vue 2.7.16 plus the other latest CDN
   libraries) and run browser tests. Treat Vue 3/Element Plus as a separate UI
   migration rather than part of the dependency-only patch.
