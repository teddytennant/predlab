# PredLab

**PredLab** is the unified paper-trading environment for the school Prediction Markets Club.

Students practice real trading strategies on **both** Polymarket-style and Kalshi-style
markets using paper money, while club admins get a fast terminal tool for onboarding and
oversight. Everything is one repo.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET OR KALSHI.** Educational use.

## 🏆 Leaderboard

Refreshed hourly by a GitHub Action from the live club server (see
[`scripts/gen_leaderboard.py`](scripts/gen_leaderboard.py)).

<!-- LEADERBOARD:START -->

_Auto-updated 2026-05-21 12:44 UTC — combined paper net worth across both simulators._

| # | Member | Polymarket | Kalshi | Total |
|--:|:-------|-----------:|-------:|------:|
| 🥇 | club_admin | $25,000.00 | $25,000.00 | **$50,000.00** |
| 🥈 | username | $25,000.00 | $25,000.00 | **$50,000.00** |
| 🥉 | trump | $25,000.00 | $25,000.00 | **$50,000.00** |
| 4 | demo_trader | $25,000.00 | $0.00 | **$25,000.00** |

<!-- LEADERBOARD:END -->

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

The TUI has three tabs (`Tab` to cycle):
- **Issue keys** — type a username, pick a role with `←/→`, `Enter` mints paper keys on
  *both* simulators, saves the member to the roster, copies a credentials block to your
  clipboard, and writes the Kalshi private key to `~/.predlab/keys/<username>.pem`.
- **Roster** — the club's students from `~/.predlab/students.db` (`↑/↓` to select, `c` to
  re-copy a member's credentials).
- **Leaderboard** — every member ranked by **combined paper net worth** across both sims
  (live; `r` refreshes).

Configure endpoints/secrets via env vars: `POLY_URL`, `KALSHI_URL`, `PREDLAB_ADMIN_SECRET`
(Polymarket admin, `X-Admin-Secret`), and `PREDLAB_KALSHI_SECRET` (Kalshi admin,
`X-Kalshi-Sim-Admin`; falls back to `CLUB_ADMIN_SECRET`).

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

### Roles

Every user has a role. Key issuance is gated on **both** sims — students cannot self-serve.

| Role     | Can do                                                            |
|----------|-------------------------------------------------------------------|
| `member` | Trade & view **their own** account only (the default).            |
| `admin`  | Issue/revoke member keys, reset balances. (e.g. the VP.)          |
| `owner`  | Everything, incl. force-resolving markets and granting roles.     |

The **master secret** (`ADMIN_SECRET` for Polymarket, `CLUB_ADMIN_SECRET` for Kalshi)
authenticates as `owner` — that's your bootstrap/break-glass. An admin/owner can also act
with their **own** key, so you can hand the VP an admin key instead of the master secret.
Only an owner may mint `admin`/`owner` keys. The `predlab` TUI has a role picker (←/→).

**Standings** — `GET /admin/leaderboard` (Polymarket) and `GET /trade-api/v2/admin/leaderboard`
(Kalshi) return every member ranked by paper net worth (cash + open positions marked to the
current price). Both are admin-gated. The TUI's **Leaderboard** tab merges them into a single
combined ranking.

### Polymarket (simple header key)

1. **Get a key** (admin only — an admin key in `POLY_API_KEY`, or the owner `X-Admin-Secret`).
   Add `&role=admin` to mint an admin key (owner only):

   ```bash
   curl -X POST "https://poly.teddytennant.com/admin/create-paper-key?username=alice&role=member" \
     -H "X-Admin-Secret: $PREDLAB_ADMIN_SECRET"
   # -> {"username":"alice","role":"member","api_key":"...","secret":"...","note":"Use api_key in POLY_API_KEY header."}
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

1. **Generate a keypair** (admin only) — returns the RSA **private key once** (save it) plus a
   key id. Authorize with the owner `X-Kalshi-Sim-Admin` secret (or an admin's signed request):

   ```bash
   curl -X POST "https://kalshi.teddytennant.com/trade-api/v2/api_keys/generate?username=alice&role=member" \
     -H "X-Kalshi-Sim-Admin: $CLUB_ADMIN_SECRET" \
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

**Access model:**
- Key issuance is **admin-gated on both sims** (see [Roles](#roles)) — students can't
  self-serve; only people you (or an admin) issue a key to can trade, and only an owner can
  mint admin/owner keys.
- Members are scoped to their own account on every endpoint.
- CORS is `allow_origins=["*"]`. Harmless for SDK/script clients; tighten if you add a
  browser frontend on a specific origin.
