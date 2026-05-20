# Phase 1 Foundation — kalshi-sim

**Date:** 2026-05-20  
**Status:** Complete and demonstrably working

## What Was Built

- Clean git repo with AGENTS.md (copied from principia-ai-homeschool) enforcing real-user authorship, concise "why" messages, no Co-Authored-By, always push after tasks.
- Full modern Python package layout (`src/kalshi_sim/`) per `docs/plans/06-python-backend.md` and the approved multi-sim plan:
  - `pyproject.toml` with ruff + mypy (strict) + pytest + all runtime deps
  - `pydantic-settings` config (DATABASE_URL, KALSHI_API_BASE, STARTING_BALANCE_CENTS, SYNC_INTERVAL etc.)
  - SQLAlchemy 2.0 models: `User`, `PaperAccount`, `Market` (ticker PK, all Kalshi string price fields), `Order`, `Trade`, `Position`, `ApiKey`
  - App factory + lifespan in `main.py`
- **Live data sync** (`services/sync.py` + `clients/kalshi_client.py`): uses `httpx` to hit the real public `https://external-api.kalshi.com/trade-api/v2/markets?limit=...&status=open` and upserts into local DB preserving exact `yes_bid_dollars`, `yes_ask_dollars`, `volume_fp`, `ticker` names etc.
- **In-memory orderbook** (`services/orderbook.py`): `OrderBook` dataclass with yes bids/asks, price-time priority, `snapshot()` returning Kalshi-style depth. Global registry for Phase 1 (Redis planned).
- **Kalshi-shaped API surface** (mounted under `/trade-api/v2`):
  - `GET /health`
  - `GET /trade-api/v2/markets?limit=20&status=open` — returns real-time synced `GetMarketsResponse` with live data
  - `GET /trade-api/v2/markets/{ticker}`
  - `GET /trade-api/v2/markets/{ticker}/orderbook`
  - `POST /trade-api/v2/portfolio/orders` (stub — creates resting order in book)
  - `POST /trade-api/v2/api_keys/generate` — RSA-2048 keypair via `cryptography`, returns private PEM once + synthetic key_id, stores record in DB (demo user auto-created)
- Pydantic response models exactly mirroring Kalshi OpenAPI (strings for prices, cursor pagination stub, etc.)
- `data/` dir + SQLite default for zero-config dev.
- Initial sync happens in lifespan so first request is populated.

Everything is documented, typed, and follows the "copy style from fastapi-server/main.py" guidance (docstrings, clean error handling, dependency injection).

## How to Run (Developer)

```bash
cd /home/gradient/all-my-repos/education/prediction-club/kalshi-sim

python -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"

cp .env.example .env
# (optional) edit PORT or DATABASE_URL

# Run
python -m kalshi_sim.main
# or
uvicorn kalshi_sim.main:app --port 8002 --reload
```

Server listens on 8002 by default (to sit beside a potential polymarket-sim on 8001).

## Proof — Live curl Examples (see verification below)

## Next Steps (from approved plan)

- **Phase 2 (API Fidelity):** Implement real RSA signature verification on every protected call (`KALSHI-ACCESS-*` headers), full paper trading service (balance checks, position updates, P&L), limit-order matching engine that actually fills against the book or auto-MM, proper order cancel/amend/batch, user-specific `/portfolio/positions` and `/fills`.
- Persist orders/trades/positions to Postgres + add Alembic migrations.
- Redis-backed orderbook + pub/sub for WebSocket channels (AsyncAPI fidelity).
- Add more Kalshi endpoints: events, series, portfolio/balance, history.
- Seed script + real user creation flow (will be driven by future Ratatui admin TUI).
- Property-based tests for matching + respx mocks of upstream.
- Black & white terminal student UI + Rust ratatui admin TUI.

See root plan.md (session 019e45ad...) for full matrix and verification checklist.

## Differences from Polymarket Sibling (when built)

- All routes under `/trade-api/v2` (not `/markets` or CLOB paths)
- Ticker-centric (not condition_id / token_id)
- RSA-PSS signature model (vs Poly's EIP-712 / Poly headers)
- Binary yes/no depth representation
- Cents vs pUSD collateral model in paper accounts
- Exact response field names (`yes_bid_dollars`, `_fp` suffixes, `event_ticker`, etc.)

This foundation is deliberately minimal, correct, and immediately runnable with real upstream data.

---

**Verification performed:** Server starts cleanly, initial sync succeeds, `curl` returns 20+ live Kalshi markets with real prices, order creation and key generation endpoints respond with correct shapes.
