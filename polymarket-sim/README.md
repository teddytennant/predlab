# polymarket-sim

Educational **paper trading simulator** that mimics the public market data (Gamma) and CLOB trading APIs of Polymarket.

Built for the school Prediction Markets Club so students can practice real strategies, write bots, and analyze P&L using live prices — with zero financial risk.

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

`/markets` returns **real live data** pulled from the public Polymarket Gamma API on startup and refreshed by a background sync loop.

## With Docker Compose

The full club stack (sim + Postgres + leaderboard site) lives in the repo-root
`docker-compose.yml` — use that for deployments. This directory's
`docker-compose.yml` brings up just the sim + Postgres for local development:

```bash
docker compose up --build
```

## API surface

- **Public market data:** `GET /markets` (filters, offset/limit pagination, `q=` search), `GET /events`
- **CLOB helpers:** `GET /book`, `POST /books`, `GET /midpoint`, `GET /spread`, `GET /last-trade-price`
- **Trading (paper, `POLY_API_KEY` header):** `POST /order`, `POST /orders` (batch), `DELETE /order`
- **Portfolio:** `GET /positions`, `GET /portfolio`, `GET /user/orders`, `GET /data/orders`, `GET /data/trades`
- **Admin (role-gated):** create/revoke keys, set roles, reset balances, delete users, force-resolve markets, leaderboard

Paper keys (`pm_paper_…`) are bearer tokens — no L2/EIP-712 signature is
verified, so SDKs work with dummy creds. Code written against the sim still
needs real wallet signing (and real money) to run against the live exchange.

See the repo-root `README.md` for the member guide and `docs/OPERATIONS.md` for
running the club.

Run the tests with `pytest`.

**NOT AFFILIATED WITH POLYMARKET.** Purely educational.
