# 拾链 Linkwise

拾链是一个自托管书签管理工具，用于收藏、整理、导入和导出链接。

当前后端运行在 Cloudflare Workers Rust + D1 上。前端仍然是非构建的静态页面，直接放在 `webapp/` 下由 Workers Static Assets 托管。

## 目标架构

```text
Cloudflare Workers Rust API
Cloudflare D1
Workers Static Assets
```

## 项目结构

```text
src/              Rust Worker 后端源码
webapp/           静态前端，无构建步骤
migrations/       Cloudflare D1 数据库迁移
wrangler.toml     Cloudflare Worker 配置
Cargo.toml        Rust Worker 工程配置
```

## Cloudflare 开发

安装依赖：

```bash
npm install
cargo install worker-build
```

创建 D1 数据库：

```bash
wrangler d1 create linkwise-db
```

设置用于保护 WebDAV 密码的 Worker secret：

```bash
wrangler secret put LINKWISE_SECRET
```

设置用于初始化第一个管理员 Passkey 的 Worker secret：

```bash
wrangler secret put LINKWISE_SETUP_TOKEN
```

生产环境启用认证前，请配置最终访问域名对应的 WebAuthn origin 和 RP ID：

```bash
wrangler secret put LINKWISE_AUTH_ORIGIN
wrangler secret put LINKWISE_AUTH_RP_ID
```

示例值：

```text
LINKWISE_AUTH_ORIGIN=https://links.example.com
LINKWISE_AUTH_RP_ID=links.example.com
```

请尽量在最终生产域名确定后再初始化管理员 Passkey。如果先在 `workers.dev` 域名初始化，之后切换到自定义域，旧 Passkey 可能无法继续使用。

应用数据库迁移：

```bash
wrangler d1 migrations apply linkwise-db --local
```

本地运行 Worker：

```bash
npm run dev
```

## Cloudflare Dashboard 部署

这个仓库可以按 Cloudflare Dashboard 的 Git 部署流程接入，形式上和常见的 npm/TypeScript Worker 仓库一致：

```text
Build command: npm run build
Deploy command: npm run deploy
```

`npm run build` 会调用 `scripts/build-worker.sh`。如果 Cloudflare 构建环境里没有 Rust，它会先安装最小 Rust toolchain，然后安装 `worker-build` 并生成 Worker 产物。

Dashboard 的 Deploy command 使用 `npm run deploy`，只部署 Build command 生成的产物，避免 `wrangler deploy` 再触发一次 Rust 构建。本地需要一键构建并部署时可以运行 `npm run deploy:full`。

Dashboard 部署前还需要完成这些配置：

1. 在 Cloudflare 创建 D1 数据库。
2. 确认 Worker 绑定了名为 `DB` 的 D1 database。
3. 在 Worker 的 Variables and Secrets 里设置 `LINKWISE_SECRET`。
4. 设置 `LINKWISE_SETUP_TOKEN`，用于注册第一个管理员 Passkey。
5. 设置 `LINKWISE_AUTH_ORIGIN` 和 `LINKWISE_AUTH_RP_ID`，并在最终生产域名确定后再初始化管理员 Passkey。

Worker 会在第一次处理 API 请求时自动初始化 D1 schema。`migrations/0001_init.sql` 仍然保留，作为 schema 的显式记录和后续数据库变更的迁移基础。

## 认证和管理模式

Linkwise 默认公开只读。任何访问者都可以浏览、搜索和导出书签；新增、编辑、删除、排序、导入、目录管理和 WebDAV 配置都需要先解锁管理模式。

首次点击“初始化管理权限”时，Linkwise 会要求输入 `LINKWISE_SETUP_TOKEN` 并注册第一个 Passkey。第一个 Passkey 注册成功后，setup token 会在逻辑上永久失效；后续解锁和新增 Passkey 都必须依赖已有管理员身份。

Web 端管理员 session 使用 HttpOnly cookie 和 D1 中保存的 token hash。默认是浏览器会话 cookie，服务端最长有效期不超过 24 小时。本地 `localhost` / `127.0.0.1` 开发允许 cookie 不带 `Secure`，生产 HTTPS 环境必须带 `Secure`。

如果所有管理员 Passkey 都丢失，第一版只支持手动 D1 运维恢复。通过 Cloudflare D1 控制台或 `wrangler d1 execute` 执行：

```sql
DELETE FROM admin_credentials;
DELETE FROM admin_sessions;
DELETE FROM auth_challenges;
DELETE FROM settings
WHERE key IN ('auth.setup_completed', 'auth.setup_completed_at');
```

然后确认 `LINKWISE_SETUP_TOKEN` 仍然存在，重新打开 Linkwise 并初始化第一个 Passkey。

当前 Rust Worker 提供这些主要 API：

```text
GET /api/health
GET /api/bootstrap
GET /api/bookmarks
GET /api/folder-orders
GET /api/bookmarks/export
GET /api/auth/status
POST /api/auth/passkey/register/options
POST /api/auth/passkey/register/verify
POST /api/auth/passkey/login/options
POST /api/auth/passkey/login/verify
POST /api/auth/logout
GET /api/auth/passkeys
DELETE /api/auth/passkeys/:credential_id
GET /api/auth/sessions
DELETE /api/auth/sessions/:session_id
POST /api/auth/sessions/revoke-all
POST /api/bookmarks
POST /api/bookmarks/bulk
POST /api/bookmarks/move
POST /api/bookmarks/reorder
POST /api/bookmarks/delete
DELETE /api/bookmarks/:id
POST /api/folders/reorder
POST /api/folders/move-up
POST /api/folders/rename
POST /api/folders/delete
GET /api/webdav/config
POST /api/webdav/config
```

其中公开只读 API 包括 `health`、`bootstrap`、`bookmarks`、`folder-orders`、`bookmarks/export` 和 `auth/status`。其他管理 API 需要有效 admin session。

WebDAV 配置已迁移到 D1。当前 Worker 版不会把 WebDAV 密码明文写入 D1；保存新密码时需要配置 `LINKWISE_SECRET`。

## 测试

基础构建验证：

```bash
cargo check
npm run build
```

后端 API 回归测试应针对运行中的 Worker 发 HTTP 请求，而不是再使用旧的进程内测试方式。先启动本地 Worker：

```bash
npm run dev
```

然后让测试请求本地 Worker，例如：

```text
http://127.0.0.1:8787/api/health
http://127.0.0.1:8787/api/bootstrap
```

测试 fixture 默认启动 `wrangler dev`，并把本地 D1 状态持久化到 `data/`。`data/` 已被 `.gitignore` 忽略，不会提交到仓库。

运行 API 回归测试：

```bash
.venv/bin/python -m pytest -m api
```

也可以指定一个已经部署好的测试环境：

```bash
LINKWISE_API_BASE_URL=https://your-test-worker.example.com .venv/bin/python -m pytest -m api
```

前端 E2E 测试仍然可以复用原来的思路：用 Playwright 打开运行中的 Worker 站点，通过 UI 创建、导入、移动、删除书签。区别是测试目标应改为本地 `wrangler dev` 或部署后的 Cloudflare Worker URL。

为避免误伤生产数据，自动化测试应使用单独的 Cloudflare D1 测试库或本地 D1。

运行 E2E 测试：

```bash
.venv/bin/python -m pytest -m e2e
```

默认测试会启动本地 `wrangler dev`。也可以指定一个已经部署好的测试环境：

```bash
LINKWISE_E2E_BASE_URL=https://your-test-worker.example.com .venv/bin/python -m pytest -m e2e
```
