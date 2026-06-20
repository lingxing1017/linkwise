FROM python:3.11-slim

WORKDIR /app

RUN pip install --no-cache-dir flask gunicorn cryptography

COPY legacy/flask/ /app/
COPY webapp/ /app/webapp/

ENV LINKWISE_WEBAPP_DIR=/app/webapp

EXPOSE 7500

CMD ["gunicorn", "-w", "2", "-b", "0.0.0.0:7500", "app:app"]
