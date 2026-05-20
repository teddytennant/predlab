# Minimal Dockerfile for polymarket-sim (Phase 1)
FROM python:3.12-slim

WORKDIR /app

# System deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

COPY pyproject.toml ./
RUN pip install --no-cache-dir -e ".[dev]" || pip install --no-cache-dir .

COPY . .

ENV PYTHONPATH=/app/src
ENV DATABASE_URL=sqlite:///./data/polymarket.db

EXPOSE 8001

CMD ["uvicorn", "polymarket_sim.main:app", "--host", "0.0.0.0", "--port", "8001"]
