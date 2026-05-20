FROM python:3.11-slim

WORKDIR /app

# 极简安装 Flask 依赖
RUN pip install --no-cache-dir flask gunicorn

# 暴露端口
EXPOSE 7500

CMD ["gunicorn", "-w", "2", "-b", "0.0.0.0:7500", "app:app"]
