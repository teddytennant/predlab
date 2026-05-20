# polymarket-sim

Educational **paper trading simulator** that faithfully mimics the public market data (Gamma) and CLOB trading APIs of Polymarket.

Built for the school Prediction Markets Club so students can practice real strategies, write bots, and analyze P&L using live prices — with zero financial risk.

**Status:** Phase 1 (Foundation) complete.

## Quick Start (Local, no Docker)

```bash
cd polymarket-sim
python -m venv .venv
source .venv/bin/activate   # or `fish` equivalent
pip install -e ".[dev]"
cp .env.example .env
# (optional) edit DATABASE_URL etc.

# Run
uvicorn polymarket_sim.main:app --reload --port 8001
```

Then:

```bash
curl http://localhost:8001/health
curl "http://localhost:8001/markets?active=true&limit=5" | python -m json.tool
```

The `/markets` endpoint returns **real live data** pulled from the public Polymarket Gamma API on startup.

## With Docker Compose (recommended for full stack)

```bash
docker compose up --build
```

(Uses Postgres + the app; currently defaults to sqlite inside container until DB_URL updated.)

## Endpoints (Phase 1)

- `GET /health`
- `GET /markets?active=true&limit=20` — real-shaped live markets
- `POST /orders` — stub paper order creation (exercises in-memory orderbook)

## Next Steps (from approved plan)

See `FOUNDATION.md` for exactly what was built and the roadmap to Phase 2 (full API fidelity, auth, paper trading engine, WebSockets).

This project follows the exact layout and Python patterns from the Principia AI Homeschool `docs/plans/06-python-backend.md` + the approved Polymarket-sim plan.

**NOT AFFILIATED WITH POLYMARKET.** Purely educational.
