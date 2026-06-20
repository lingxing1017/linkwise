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

Worker 会在第一次处理 API 请求时自动初始化 D1 schema。`migrations/0001_init.sql` 仍然保留，作为 schema 的显式记录和后续数据库变更的迁移基础。

当前 Rust Worker 提供这些主要 API：

```text
GET /api/health
GET /api/bootstrap
GET /api/bookmarks
POST /api/bookmarks
POST /api/bookmarks/bulk
POST /api/bookmarks/move
POST /api/bookmarks/reorder
POST /api/bookmarks/delete
DELETE /api/bookmarks/:id
GET /api/folder-orders
POST /api/folders/reorder
POST /api/folders/move-up
POST /api/folders/rename
POST /api/folders/delete
GET /api/bookmarks/export
GET /api/webdav/config
POST /api/webdav/config
```

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

前端 E2E 测试仍然可以复用原来的思路：用 Playwright 打开运行中的 Worker 站点，通过 UI 创建、导入、移动、删除书签。区别是测试目标应改为本地 `wrangler dev` 或部署后的 Cloudflare Worker URL。

为避免误伤生产数据，自动化测试应使用单独的 Cloudflare D1 测试库或本地 D1。
