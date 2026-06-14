FROM python:3.11-slim

WORKDIR /app

RUN pip install --no-cache-dir flask gunicorn cryptography

COPY app/ /app/

EXPOSE 7500

CMD ["gunicorn", "-w", "2", "-b", "0.0.0.0:7500", "app:app"]
