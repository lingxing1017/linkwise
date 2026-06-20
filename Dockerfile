FROM python:3.11-slim

WORKDIR /app

RUN pip install --no-cache-dir flask gunicorn cryptography

COPY src/ /app/src/
COPY webapp/ /app/webapp/

EXPOSE 7500

CMD ["gunicorn", "-w", "2", "-b", "0.0.0.0:7500", "--chdir", "/app/src", "app:app"]
