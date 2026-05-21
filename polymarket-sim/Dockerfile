# Minimal Dockerfile for polymarket-sim (Phase 1)
FROM python:3.12-slim

WORKDIR /app

# System deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the full project before installing: pyproject sets readme = "README.md"
# and finds packages under src/, so both must be present when the build backend
# resolves metadata (a pyproject-only copy fails at "getting requirements").
COPY . .
RUN pip install --no-cache-dir -e ".[dev]"

ENV PYTHONPATH=/app/src
ENV DATABASE_URL=sqlite:///./data/polymarket.db

EXPOSE 8001

CMD ["uvicorn", "polymarket_sim.main:app", "--host", "0.0.0.0", "--port", "8001"]
