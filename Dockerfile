FROM python:3.11-slim

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements and install
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application files
COPY auriga_core.py feed_store.py database.py app.py .
COPY templates/ ./templates/
COPY static/ ./static/

# Environment defaults
ENV PORT=5000
ENV HOST=0.0.0.0
ENV FEEDS_DB_PATH=/app/data/feeds.db

RUN mkdir -p /app/data

EXPOSE 5000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:5000/health || exit 1

CMD ["python3", "app.py"]
