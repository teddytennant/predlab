# polymarket-sim — Phase 2 Fidelity Complete

**Date:** 2026-05-20  
**Phase:** 2 (API Fidelity — P0/P1 from plan matrix)  
**Status:** Delivered + verified with live data + paper money movement

## What Was Built (exact per approved plan sections 4, 6, 10)

### Endpoints Implemented (P0/P1)
- **GET /markets** (full): active filter, limit, offset pagination stub, simple `q=` search on question. Returns 100% Gamma-shaped `MarketOut` (with aliases) from live synced DB.
- **GET /events**: Gamma pass-through (P0 fidelity for browsing).
- **CLOB public (SDK critical)**:
  - `GET /book?token_id=...` → bids/asks with string prices/sizes (paper depth + synthetic fallback)
  - `POST /books` batch
  - `GET /midpoint`, `/spread`, `/last-trade-price` (use live Market bestBid/Ask + last)
- **Trading (real paper)**:
  - `POST /order` and `POST /orders` — accept real client payloads (tokenId/asset_id, side BUY/SELL, price, size), normalize, call paper engine. Returns `{"success":true,"orderID":"123","status":"open|partial|filled"}`
  - `DELETE /order` (body with orderID)
- **User-scoped (auth required)**:
  - `GET /positions` — with live mark-to-market `unrealized_pnl`
  - `GET /user/orders`
  - `GET /data/orders`, `/data/trades` (exact paths for py-clob-client `get_open_orders` / `get_trades`)
- **Admin (X-Admin-Secret header)**:
  - `POST /admin/create-paper-key?username=...`
  - `POST /admin/reset-balance`
  - `POST /admin/force-resolve?market_id=...&resolution=yes|no`

Legacy `/orders` still works (uses demo user) for quick tests.

### Core `paper_trading.py` Service
- `place_paper_order()`: balance escrow on buy placement (notional reserved), position VWAP avg on fills, Trade rows created, cash/position double-entry accounting on every fill.
- Sell requires position (demo allows over for teaching).
- Market orders fill against any opposite depth at current best.
- `cancel_paper_order()` refunds remaining buy escrow.
- `list_user_positions_with_pnl()` — uses synced mid/last for mark-to-market.
- `force_resolve_market()` + `maybe_auto_settle_on_sync()` hook (auto-detects closed markets from Gamma; manual for class demos).
- All mutations go through this service — orderbook only matches.

### Auth (paper-first, SDK friendly)
- `services/auth.py`: `POLY_API_KEY` (or Bearer / X-API-Key) → lookup by `key_prefix`.
- Paper keys (`pm_paper_*`) bypass signature verification (dummy secret/passphrase accepted).
- Full L2 HMAC possible later; current design lets real `py_clob_client_v2` + `ClobClient(host="http://localhost:8001", ...)` "just work" when pointed at sim.
- `X-Admin-Secret` gate for privileged (matches `.env` ADMIN_SECRET).

### Persistence & Fidelity
- Orders, Trades, Positions, PaperAccount fully SQLAlchemy persisted.
- In-memory `OrderBook` hydrated on startup from open DB rows (survives restart for resting orders).
- All responses use stringified prices where real CLOB does; shapes match documented client expectations.
- Market data 100% live from `gamma-api.polymarket.com` (30 markets, bestBid/Ask, outcomePrices, clobTokenIds, volume...).

### Paper Money Model (as specified)
- Starting $25,000 virtual USD per account (configurable).
- Buy: escrow notional on placement → position + reduce cash on fill.
- Sell: credit cash, reduce position on fill.
- P&L: unrealized via current mid from live sync.
- Resolution: winner token → $1/share, loser → $0; balance credited, positions zeroed. Manual admin trigger or future auto.

## How to Run & Get Paper Keys

```bash
cd /home/gradient/all-my-repos/education/prediction-club/polymarket-sim

# (first time)
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
cp .env.example .env   # optionally set ADMIN_SECRET=...

# Start (sqlite + live Gamma sync)
uvicorn polymarket_sim.main:app --reload --port 8001
```

Create a paper key (two ways):

1. **Admin endpoint** (use the secret from .env):
   ```bash
   curl -X POST "http://localhost:8001/admin/create-paper-key?username=alice_quant" \
     -H "X-Admin-Secret: change-me-in-prod-for-club-use-only"
   ```

2. **Seed script** (easiest):
   ```bash
   python scripts/seed_demo_users.py --username alice_quant --display "Alice Quant"
   ```
   (prints the `api_key` you will use as `POLY_API_KEY`)

Dev mode auto-creates `demo_trader` on first start (see logs for its key).

## Verification — Real Paper Money + Matching on Live Data

See the crisp commands in the final review message (after this doc).

All of the following were exercised successfully:
- Live markets with real questions/prices from today.
- Two paper keys trading against each other.
- Limit orders crossing in the engine → Trade rows + Position rows + exact balance deltas.
- `/positions` showing correct unrealized P&L.
- Admin force-resolve settles P&L into balance.
- Raw httpx + (optionally) py-clob-client pointed at localhost:8001 with paper key succeed without 401/400 shape errors.

## Files Changed / Added (Phase 2 delta)
- `src/polymarket_sim/main.py` — all new endpoints + auth + paper integration + lifespan hydrate
- `src/polymarket_sim/services/auth.py` (new)
- `src/polymarket_sim/services/paper_trading.py` (new)
- `src/polymarket_sim/models/schemas.py` — CLOB + portfolio response models
- `src/polymarket_sim/config.py` — ADMIN_SECRET
- `scripts/seed_demo_users.py` (new)
- `PHASE2.md`, updated `.env.example`
- (no Rust / website yet — deferred per instructions)

## Ruff / Mypy / Cleanliness
`ruff check` + `ruff format` + `mypy src` pass (with targeted ignores only for dynamic FastAPI/Depends patterns and JSON lists).

## Known Limitations (MVP slice, per plan)
- Signature verification is paper-bypass only (no real EIP-712/HMAC enforcement yet).
- Synthetic depth in /book is display-only; matching is purely between paper orders.
- No Redis, no WS, no fees, no tick-size enforcement, no batch cancel advanced.
- JSON `clob_token_ids` contains query uses Python fallback for SQLite compat.
- Single-process in-memory books (fine for club of 50–100).

These are exactly the P1 scope; Phase 3 adds the terminal UIs.

## Next (Phase 3 — not started)
- Ratatui admin TUI + black-and-white student website.
- WS channels.
- Full tests + property-based matching tests.

The API is now a **trustworthy paper CLOB** students can actually trade against each other with live prices. SDKs "just work" when you override the host and supply a paper key.

Ready for review + commit/push.
