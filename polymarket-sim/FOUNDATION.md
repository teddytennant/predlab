# polymarket-sim — Phase 1 Foundation Complete

**Date:** 2026-05-20  
**Phase:** 1 (Foundation)  
**Repo:** polymarket-sim (the club now runs Polymarket-only; Kalshi sim was removed in the 2026 migration)

## What Was Built

This deliverable implements the **exact** tasks specified for Phase 1 of the approved plan:

1. **Clean git repository**
   - `git init` performed
   - AGENTS.md copied verbatim from the homeschool project (enforces real-user authorship, concise "why" commits, no Co-Authored-By, always push after review)

2. **Standard Python project skeleton** (following `docs/plans/06-python-backend.md` + approved layout)
   - `pyproject.toml` with:
     - Modern `[project]` metadata + dependencies
     - `ruff`, `mypy` (strict), `pytest` + asyncio config
     - `src/` layout with proper package `polymarket_sim`
   - `src/polymarket_sim/` with subpackages: `api/`, `models/`, `services/`, `clients/`, `utils/`
   - `tests/`, `scripts/`, `alembic/` (ready for later)
   - `.env.example`, `docker-compose.yml`, `Dockerfile`
   - `data/` for dev sqlite

3. **Basic FastAPI application**
   - App factory: `create_app()` + lifespan
   - Pydantic-Settings `config.py` (DATABASE_URL, gamma base, sync interval, starting balance, etc.)
   - SQLAlchemy 2.0 models (`models/db.py`):
     - `User`, `PaperAccount`, `Market`, `Order`, `Trade`, `Position`, `ApiKey`
     - Proper relationships, indexes, JSON columns for outcomes/prices/tokens, Numeric for money/quantities
   - `db.py`: engine, `SessionLocal`, `init_db()`, `get_session` dependency
   - Health endpoint + structured logging

4. **Stub data sync service**
   - `clients/gamma.py`: clean async httpx wrapper
   - `services/sync.py`: `sync_markets_from_gamma()` that:
     - Calls `https://gamma-api.polymarket.com/markets?limit=20&active=true`
     - Parses JSON strings for outcomes/outcomePrices
     - Upserts ~20 real active markets with `bestBid`, `bestAsk`, `lastTradePrice`, `volume`, `liquidity`, `clobTokenIds`, dates, etc.
     - Runs automatically at startup (one-shot for Phase 1)

5. **In-memory orderbook + basic matching**
   - `services/orderbook.py`:
     - `OrderBook` dataclass with bids/asks lists (price-time priority)
     - `add_limit_order()` performs crossing matching for buys vs asks / sells vs bids
     - `get_orderbook(token_id)` global registry (in-memory; comment says "later Redis")
     - `snapshot()` for future orderbook endpoints
   - Exercised by the stub order creation path

6. **Exposed endpoints (minimum required)**
   - `GET /health`
   - `GET /markets` — returns **real-shaped** data directly from the synced DB (fields match Gamma closely via Pydantic `MarketOut` with aliases)
   - `POST /orders` — creates a paper `Order` row, attempts matching against the in-memory book, updates status/filled_size (full paper balance/position logic deferred to Phase 2)

7. **Documentation**
   - `README.md` (quick start)
   - This `FOUNDATION.md`
   - All code has docstrings and follows the "documented code" requirement

8. **Runnable + proven**
   - `uvicorn polymarket_sim.main:app --port 8001` works immediately (sqlite)
   - Also supports `python -m` style via the `__main__` block
   - Verified via terminal: curl shows live markets with real prices, outcomePrices, bestBid/Ask coming through the DB sync

## How to Run

### Fastest (recommended for Phase 1 review)

```bash
cd /home/gradient/all-my-repos/education/prediction-club/polymarket-sim

# One-time
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"

cp .env.example .env   # uses sqlite by default

# Run the server
uvicorn polymarket_sim.main:app --reload --port 8001
```

### Alternative direct module

```bash
PYTHONPATH=src python -m uvicorn polymarket_sim.main:app --port 8001
```

### Docker (future full stack)

```bash
docker compose up --build
# (currently sqlite inside; swap DATABASE_URL for postgres when ready)
```

## Verification Commands (run these to prove it)

After server is up:

```bash
curl -s http://localhost:8001/health | python -m json.tool

curl -s "http://localhost:8001/markets?limit=3" | python -c '
import sys, json
data = json.load(sys.stdin)
print("Markets returned:", len(data))
for m in data[:2]:
    print(m["question"][:70], "bid/ask:", m.get("bestBid"), m.get("bestAsk"))
'

# Stub order (creates a DB row + exercises orderbook)
curl -s -X POST http://localhost:8001/orders \
  -H "content-type: application/json" \
  -d '{"market_id":"540817","side":"buy","price":0.40,"size":10}' | python -m json.tool
```

Sample output from real run (live data):

- Real questions such as "New Rihanna Album before GTA VI?", prices ~0.51/0.49, bestBid/bestAsk populated, volume numbers from the real platform, etc.

## Next Steps (per approved plan)

**Phase 2 (API Fidelity):**
- Real API key auth (`ApiKey` model + validation middleware)
- Full paper trading service (balance checks, position updates, P&L, fills → Trade + Position rows)
- Proper CLOB endpoints: `/orderbook/{token}`, `/midpoint`, `/last-trade-price`, batch orders, cancel
- User-scoped data under auth
- Error handling, pagination, rate limits
- WebSocket skeleton (FastAPI + Redis pubsub later)
- Seed script for demo users

**Later:**
- Alembic migrations (currently `create_all`)
- Background recurring sync (APScheduler)
- Redis-backed orderbook + orderbook snapshots
- Real signature verification (Polymarket CLOB style headers)
- The full student terminal UI + Ratatui admin TUI
- Tests (respx for Gamma, property tests for matching engine)

## Design Notes & Fidelity

- Database uses SQLite for instant local start; production path (docker) is Postgres-ready.
- All money/quantity columns use `Numeric` (exact decimal).
- Orderbook is deliberately simple and in-memory so the foundation is **runnable today** while the matching logic is already real (price-time, partial fills).
- Market data is 100% from the live public API — students will see the exact same questions and prices as on Polymarket.com right now.
- Every response and the README clearly labels this as a **PAPER TRADING SIMULATOR**.

The foundation is solid, documented, and demonstrably working with real data flowing through the entire stack (Gamma → DB → API → orderbook engine).

Ready for Phase 2 and review.

**Phase 2 Fidelity delivered** (see PHASE2.md). The simulator now has full paper CLOB trading, auth, P&L, admin controls, and SDK-compatible shapes.
