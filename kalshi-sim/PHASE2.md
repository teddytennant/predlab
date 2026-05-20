# Phase 2 — API Fidelity & Paper Trading Engine (kalshi-sim)

**Date:** 2026-05-20  
**Status:** Complete — solid for club strategy testing. (TUI + B&W site in Phase 3)

## Delivered (per approved plan §10 Kalshi matrix + §4/6/2)

### Endpoints under real `/trade-api/v2` (SDK drop-in)
- **P0 Public**
  - `GET /trade-api/v2/markets?limit&status&event_ticker&...` — richer filters, live synced prices + `simulated: true`
  - `GET /trade-api/v2/events` — basic event grouping from markets
  - `GET /trade-api/v2/markets/{ticker}/orderbook` — **exact** production shape `{"orderbook_fp": {"yes_dollars": [[p,s],...], "no_dollars":[...]}}`
- **P1 Authenticated (RSA or dev-bypass)**
  - `POST /trade-api/v2/portfolio/orders` (and `/portfolio/events/orders` alias) — **V2 shape** (`bid`/`ask`, `client_order_id`, fp strings for `count`/`price`, tif, stp, etc.)
  - `DELETE /trade-api/v2/portfolio/orders/{id}`
  - `GET /trade-api/v2/portfolio/orders`
  - `GET /trade-api/v2/portfolio/balance` (cents + dollars fp, portfolio_value)
  - `GET /trade-api/v2/portfolio/positions` (market_positions with signed `position_fp`)
  - `GET /trade-api/v2/portfolio/fills`
- **Privileged admin (header `X-Kalshi-Sim-Admin: $CLUB_ADMIN_SECRET`)**
  - `POST /trade-api/v2/admin/reset-user?username=...` — zero positions, cancel orders, restore starting balance
  - `POST /trade-api/v2/admin/resolve/{ticker}?result=yes|no` — force settlement + payout into balances (teaching tool)

All responses match Kalshi OpenAPI shapes so official SDKs (`kalshi_python_sync` etc.) work with `host=` override.

### Core Paper Trading Engine (`services/paper_trading.py`)
- `PaperTradingService` with:
  - Real matching: price-time priority limit orders crossing the in-mem book (populated from persisted `Order` rows on startup via `rebuild_books_from_db`)
  - Balance in **cents**, signed `yes_contracts` positions (long yes >0, short yes <0)
  - On fill: immediate cash leg (debit/credit balance), position delta, `Trade` rows for both sides, avg fill price
  - TIF handling (GTC rest, IOC/FOK cancel remainder), basic self-trade prevention (`taker_at_cross`)
  - P&L / settlement on admin `force_resolve` (or future status sync): payout 100¢ or 0¢ per contract into `PaperAccount.balance_cents`, zero positions, cancel residual orders
- User-scoped queries only (via auth-resolved `user_id`)
- Persisted to SQLite (Postgres ready); in-mem books for hot path + DB authoritative

### Real RSA-PSS Verification
- `api/auth.py`: `_verify_signature` + `require_signed_auth` (exact `{ts}{METHOD}{path_no_query}` + PSS/SHA256/DIGEST_LENGTH, base64)
- `POST /trade-api/v2/api_keys/generate` now stores `public_key_pem` in `ApiKey`, returns private PEM once
- `dev_bypass_auth` (env `DEV_BYPASS_AUTH=true`, default for dev) + optional `X-Kalshi-Sim-User` header for multi-user tests without signing
- When `false`: every protected call (orders, portfolio, cancel) enforces valid signature or 401
- Compatible with SDK signing (the same private PEM + key_id + headers the SDK would send to real Kalshi)

### Other
- Updated DB models (ApiKey pubkey, Order V2 fields, Market.result for settlement)
- Richer Pydantic V2 request/response models exactly mirroring Kalshi
- Rebuild books from DB on startup/lifespan
- Admin secret from config (default in .env.example)
- All via real FastAPI + SQLAlchemy sessions, no stubs left for core flows

## Verification Performed (live)
(See crisp commands in the handoff summary below — they demonstrate:)
- Live Kalshi markets/prices via `/markets`
- Key generation (returns real RSA private)
- Paper bid + opposing ask (different users) **cross-fill** at maker price
- Balance delta (cents), position update (+10 yes), fills recorded, orderbook cleared
- Admin reset + (optional) force resolve
- Exact shapes (orderbook_fp, V2 create response, balance in cents, etc.)

## How to Use with Official SDK (example)
```python
# After generating a key via /api_keys/generate (save the private PEM + note the api_key_id)
from kalshi_python_sync import Configuration, KalshiClient

config = Configuration(
    host="http://localhost:8002/trade-api/v2",  # point at sim (or your deployed URL)
)
with open("my-sim-key.pem") as f:
    config.private_key_pem = f.read()
config.api_key_id = "ks_live_xxxx..."   # from generate response

client = KalshiClient(config)
bal = client.get_balance()
print(bal.balance)  # cents

# place paper order (will hit our matching engine)
order = client.create_order(...)  # V2 shape
```

Turn `DEV_BYPASS_AUTH=false` in .env for full RSA end-to-end (SDK will just sign as usual).

## Next (Phase 3)
- Rust ratatui admin TUI (create users/keys, leaderboard by P&L, force resolve, mass resets)
- B&W terminal student website (static or thin FastAPI-served, calls the real endpoints)
- WS channels (AsyncAPI fidelity) + Redis
- More markets (series, history), tests, docker, seed scripts

**This slice makes kalshi-sim fully usable for Prediction Markets Club strategy development, backtesting, and live paper trading against real Kalshi data.**

See also: FOUNDATION.md, AGENTS.md, the root plan.md (session 019e45ad...).
