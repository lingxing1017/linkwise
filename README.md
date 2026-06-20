# 拾链 Linkwise

拾链是一个自托管书签管理工具，用于收藏、整理、导入和导出链接。

当前 `worker-rust` 分支正在把后端从 Flask + SQLite 迁移到 Cloudflare Workers Rust + D1。前端仍然是非构建的静态页面，直接放在 `webapp/` 下由 Workers Static Assets 托管。

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
legacy/flask/     旧 Flask 后端，迁移期保留作对照
tests/            旧 Flask 回归测试，迁移期保留
Dockerfile        旧 Flask 镜像构建，迁移期保留
compose.yml       旧 Flask Docker Compose，迁移期保留
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

当前 Rust Worker 已迁移主要 API：

```text
GET /api/health
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

## Legacy Flask

迁移期间，旧 Flask 服务仍可作为对照运行：

```bash
python3 -m venv .venv
.venv/bin/pip install flask cryptography pytest
.venv/bin/python legacy/flask/app.py
```

旧服务默认仍使用本地 SQLite 和密钥文件：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `LINKWISE_DB_DIR` | `data` | SQLite 数据库目录 |
| `LINKWISE_SECRET_FILE` | `/run/secrets/linkwise_secret_key` | WebDAV 密码加密密钥文件 |
| `LINKWISE_WEBAPP_DIR` | `webapp` | 静态前端目录 |
| `LINKWISE_VERSION` | `dev` | 应用版本号 |

运行旧 Flask 回归测试：

```bash
.venv/bin/python -m pytest
```
