# PredLab

**PredLab** is the unified paper-trading environment for the school Prediction Markets Club.

Students practice real trading strategies on **both** Polymarket-style and Kalshi-style
markets using paper money, while club admins get a fast terminal tool for onboarding and
oversight. Everything is one repo.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET OR KALSHI.** Educational use.

## Repository structure

```
predlab/
├── polymarket-sim/   # Polymarket Gamma + CLOB API mock (Python / FastAPI)   :8001
├── kalshi-sim/       # Kalshi Trade API v2 mock        (Python / FastAPI)    :8002
├── ratatui-admin/    # Admin TUI (Rust): issue dual keys + club roster
├── docker-compose.yml
└── Makefile
```

The two simulators sync live prices from the real public APIs and expose drop-in–compatible
endpoints, so students point the official SDKs at `localhost` and only change the base URL.

## Quick start

### Get everything (one-liner)

Clone the repo and bring up the whole stack (Postgres + both simulators):

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab && docker compose up --build
```

That runs **your own local copy** at `http://localhost:8001` (Polymarket) and
`http://localhost:8002` (Kalshi). To instead use the **club's hosted server**, you don't
need to clone anything — see [Using the API](#using-the-api) for the public URLs.

### 1. Run the simulators

```bash
docker compose up --build      # Polymarket -> :8001, Kalshi -> :8002
```

Or run one directly for development:

```bash
cd polymarket-sim && pip install -e ".[dev]" && uvicorn polymarket_sim.main:app --port 8001
cd kalshi-sim     && pip install -e ".[dev]" && uvicorn kalshi_sim.main:app     --port 8002
```

### 2. Build the admin TUI

```bash
make install-admin     # cargo install --path ratatui-admin  -> `predlab` on your PATH
predlab                # or: make admin
```

The TUI has two views (`Tab` to switch):
- **Issue keys** — type a username, `Enter` mints paper keys on *both* simulators and saves
  the member to the roster.
- **Roster** — the club's students from `~/.predlab/students.db`.

Configure endpoints/secret via env vars: `POLY_URL`, `KALSHI_URL`, `PREDLAB_ADMIN_SECRET`
(the Polymarket admin endpoint is gated by `X-Admin-Secret`).

## Using the API

The club runs a **hosted instance** you can reach from anywhere — no download required,
just an API key:

| Platform   | Base URL                          | Real-API paths it mimics            |
|------------|-----------------------------------|-------------------------------------|
| Polymarket | `https://poly.teddytennant.com`   | Gamma + CLOB (`/markets`, `/order`) |
| Kalshi     | `https://kalshi.teddytennant.com` | Trade API v2 (`/trade-api/v2/...`)  |

Point the official Polymarket / Kalshi SDKs at these base URLs (or `http://localhost:8001`
/ `:8002` for a local copy) and trade with paper money.

### What's open vs. what needs a key

- **Public, no auth** — market data: `GET /health`, `GET /markets`, `GET /events`,
  `GET /book`, `GET /midpoint`, `GET /spread`, `GET /last-trade-price`.
- **Requires a key** — anything that touches an account: placing/cancelling orders,
  positions, balances. Unauthenticated calls get `401`.

### Polymarket (simple header key)

1. **Get a key** (club admin only — gated by `X-Admin-Secret`):

   ```bash
   curl -X POST "https://poly.teddytennant.com/admin/create-paper-key?username=alice" \
     -H "X-Admin-Secret: $PREDLAB_ADMIN_SECRET"
   # -> {"username":"alice","api_key":"...","secret":"...","note":"Use api_key in POLY_API_KEY header."}
   ```

   (Admins normally do this from the `predlab` TUI, which mints keys on both platforms at once.)

2. **Read public market data** (no key):

   ```bash
   curl https://poly.teddytennant.com/markets
   ```

3. **Trade** — pass your key in the `POLY_API_KEY` header (or `Authorization: Bearer <key>`):

   ```bash
   curl -X POST https://poly.teddytennant.com/order \
     -H "POLY_API_KEY: <your_api_key>" -H "Content-Type: application/json" \
     -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'

   curl https://poly.teddytennant.com/positions -H "POLY_API_KEY: <your_api_key>"
   ```

### Kalshi (RSA-signed requests)

Kalshi mirrors the real API's signed-request auth, so use the **official Kalshi Python SDK**
pointed at the base URL rather than raw curl.

1. **Generate a keypair** — returns the RSA **private key once** (save it) plus a key id:

   ```bash
   curl -X POST "https://kalshi.teddytennant.com/trade-api/v2/api_keys/generate?username=alice" \
     -H "Content-Type: application/json" -d '{"name":"alice-laptop","scopes":["trade"]}'
   # -> {"api_key_id":"ks_live_...","private_key":"-----BEGIN RSA PRIVATE KEY----- ..."}
   ```

2. **Trade** — every protected request must carry `KALSHI-ACCESS-KEY`,
   `KALSHI-ACCESS-TIMESTAMP`, and `KALSHI-ACCESS-SIGNATURE` (RSA-PSS over
   `timestamp + method + path`). The SDK builds these for you:

   ```python
   from kalshi_python import KalshiClient   # official SDK
   client = KalshiClient(
       base_url="https://kalshi.teddytennant.com/trade-api/v2",
       key_id="ks_live_...",
       private_key_pem=open("kalshi_key.pem").read(),
   )
   print(client.get_balance())
   ```

## Testing

```bash
make test          # runs all three suites
# or individually:
make test-sims     # pytest for both simulators
make test-admin    # cargo test for the admin tool
```

The simulator tests run fully offline (isolated temp SQLite, no live network sync). See
each project's `tests/` for details.

## Student experience

Each student gets one username and a paper key per platform, then:
- points the official Polymarket/Kalshi SDKs at the sims (base-URL override),
- trades paper money on either or both market designs,
- competes on the club leaderboard.

## Deploying for the club

The live instance runs on a NixOS host as a `docker compose` stack (Postgres + both sims),
exposed over HTTPS by a **Cloudflare Tunnel** (declarative `services.cloudflared` in
`/etc/nixos`). The tunnel dials out to Cloudflare, so no inbound ports are opened.
Production config lives in a gitignored `.env` (see `.env.example`):

```bash
cp .env.example .env        # set ADMIN_SECRET, CLUB_ADMIN_SECRET; keep DEV_BYPASS_AUTH=false
docker compose up -d --build
```

Update a running deployment with `git pull --ff-only && docker compose up -d --build`
(the Postgres volume persists paper balances across rebuilds).

**Access model / known gaps:**
- **Polymarket** key issuance is admin-only (`X-Admin-Secret`), so only people you hand a
  key to can trade.
- **Kalshi** `/api_keys/generate` is currently **open** — anyone who can reach the URL can
  self-mint a key and trade. Fine for an open club; gate it behind the admin secret if you
  want Kalshi to be invite-only too.
- CORS is `allow_origins=["*"]`. Harmless for SDK/script clients; tighten if you add a
  browser frontend on a specific origin.
