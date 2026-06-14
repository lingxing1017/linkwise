# 拾链 Linkwise

拾链是一个自托管的书签管理工具，用于收藏、整理、导入和导出链接。

## 功能

- 新增、编辑、删除书签
- 使用多级目录整理书签
- 支持批量导入浏览器书签 HTML
- 支持导出浏览器书签 HTML
- 支持目录和书签拖拽排序
- 支持批量移动和批量删除
- 支持 WebDAV 配置，密码会加密保存
- 支持 Docker 部署

## 界面截图

待补充。

## 快速开始

建议先下载 GitHub Actions 生成的镜像 tar 包，导入后再启动服务。

### 导入镜像

从 GitHub Actions 的 Artifacts 下载并解压镜像 tar 包：

```bash
docker load -i linkwise-YYYY.MM.DD.tar
```

导入完成后，本地会得到：

```text
linkwise:latest
```

### 准备数据和密钥

```bash
mkdir -p data secrets
openssl rand -hex 32 > secrets/linkwise_secret_key
```

如果你的环境没有 `openssl`，也可以用 Python 生成密钥：

```bash
python3 -c "import secrets; print(secrets.token_urlsafe(32))" > secrets/linkwise_secret_key
```

> [!WARNING]
> 迁移服务器时，请同时备份 `data/` 和 `secrets/linkwise_secret_key`。如果密钥文件丢失，已经保存的 WebDAV 密码将无法解密。

### 启动服务

推荐使用仓库中的 `compose.yml` 启动：

```bash
docker compose up -d
```

也可以不使用 Compose，直接运行容器：

```bash
mkdir -p data secrets
openssl rand -hex 32 > secrets/linkwise_secret_key

docker run -d \
  --name linkwise-app \
  -p 7500:7500 \
  -v "$PWD/data:/app/data" \
  -v "$PWD/secrets/linkwise_secret_key:/run/secrets/linkwise_secret_key:ro" \
  --restart always \
  linkwise:latest
```

### 访问服务

```text
http://localhost:7500
```

## 配置

应用支持以下环境变量：

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `LINKWISE_DB_DIR` | `data` | SQLite 数据库目录 |
| `LINKWISE_SECRET_FILE` | `/run/secrets/linkwise_secret_key` | WebDAV 密码加密密钥文件 |
| `LINKWISE_VERSION` | `dev` | 应用版本号 |

## 开发

### 项目结构

```text
app/        应用代码和前端资源
data/       本地运行数据，默认不提交
secrets/    本地密钥文件，默认不提交
tests/      回归测试
Dockerfile  镜像构建配置
compose.yml Docker Compose 部署配置
```

### 本地运行

建议使用虚拟环境安装依赖：

```bash
python3 -m venv .venv
.venv/bin/pip install flask cryptography pytest
```

本地运行开发服务：

```bash
.venv/bin/python app/app.py
```

### 运行测试

```bash
.venv/bin/python -m pytest
```

### 构建镜像

如果你需要自行修改或构建镜像，可以从源码构建：

```bash
git clone <your-repository-url>
cd linkwise
docker build -t linkwise:latest .
```

构建完成后，可以按上面的 Docker Compose 或 `docker run` 方式启动。
