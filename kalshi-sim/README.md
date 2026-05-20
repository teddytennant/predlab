# kalshi-sim

**Kalshi Trade API v2 Simulator** — Paper trading environment for the school Prediction Markets Club.

Live market data synced from the real Kalshi public endpoints. Paper money only. Full API fidelity under `/trade-api/v2/...` so official SDKs and bots can point here with zero (or minimal) changes for testing strategies.

> **PAPER TRADING SIMULATOR — NOT AFFILIATED WITH KALSHI**

## Phase 1 Status: Foundation Complete

See [FOUNDATION.md](./FOUNDATION.md) for build details, run commands, and curl proof.

## Quick Start (dev)

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
cp .env.example .env
# (edit if needed)
python -m kalshi_sim.main   # or uvicorn kalshi_sim.main:app --port 8002
```

Then:

```bash
curl http://localhost:8002/health
curl "http://localhost:8002/trade-api/v2/markets?limit=5&status=open"
```

## Endpoints (MVP)

- `GET /health`
- `GET /trade-api/v2/markets` — Kalshi-shaped, live synced yes_bid/ask etc.
- `POST /trade-api/v2/api_keys/generate` (stub) — returns RSA keypair (local storage)
- Order stubs under `/trade-api/v2/portfolio/orders` etc.

Next phases: full auth verification, matching engine, paper trading service, WS, terminal UI, Ratatui admin TUI.

## Project Layout

Follows the approved plan + `docs/plans/06-python-backend.md` patterns from the sibling Principia project.

See plan at the education/prediction-club session.
