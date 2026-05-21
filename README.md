# PredLab

**PredLab** is the paper-trading playground for the school Prediction Markets Club.

Trade on **both** Polymarket-style and Kalshi-style markets with fake money, practice real
strategies, and climb the club leaderboard. Nothing to install — your admin gives you a key
and you trade against the club's hosted servers.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET OR KALSHI.** Educational use, fake money.

## 🏆 Leaderboard

Refreshed hourly from the live club server.

<!-- LEADERBOARD:START -->

_Auto-updated 2026-05-21 12:44 UTC — combined paper net worth across both simulators._

| # | Member | Polymarket | Kalshi | Total |
|--:|:-------|-----------:|-------:|------:|
| 🥇 | club_admin | $25,000.00 | $25,000.00 | **$50,000.00** |
| 🥈 | username | $25,000.00 | $25,000.00 | **$50,000.00** |
| 🥉 | trump | $25,000.00 | $25,000.00 | **$50,000.00** |
| 4 | demo_trader | $25,000.00 | $0.00 | **$25,000.00** |

<!-- LEADERBOARD:END -->

## Getting started

You don't download or run anything. A club admin sets you up with:

- a **username**,
- a **Polymarket API key** (a string), and
- a **Kalshi key id** + a **private key file** (`<you>.pem` — save it, it's only shown once).

Everyone starts with **$25,000 of paper money on each platform**. You trade against the
club's hosted servers:

| Platform   | Base URL                          | What it mimics                      |
|------------|-----------------------------------|-------------------------------------|
| Polymarket | `https://poly.teddytennant.com`   | Gamma + CLOB (`/markets`, `/order`) |
| Kalshi     | `https://kalshi.teddytennant.com` | Trade API v2 (`/trade-api/v2/...`)  |

Point the official Polymarket / Kalshi SDKs (or plain `curl`) at those base URLs and trade.

**What needs your key:**

- **No key needed** — looking at market data: `GET /health`, `GET /markets`, `GET /events`,
  `GET /book`, `GET /midpoint`, `GET /spread`, `GET /last-trade-price`.
- **Needs your key** — anything that touches *your account*: placing/cancelling orders,
  positions, balance. Calls without a valid key get rejected (`401`).

### Trade on Polymarket

Polymarket just wants your key in a header. Browse first, then trade:

```bash
# 1. See what markets exist (no key needed)
curl https://poly.teddytennant.com/markets

# 2. Place an order — your key goes in the POLY_API_KEY header
curl -X POST https://poly.teddytennant.com/order \
  -H "POLY_API_KEY: <your_api_key>" -H "Content-Type: application/json" \
  -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'

# 3. Check your positions / balance
curl https://poly.teddytennant.com/positions -H "POLY_API_KEY: <your_api_key>"
```

(`Authorization: Bearer <key>` works too if your SDK prefers it.)

### Trade on Kalshi

Kalshi signs every request with your **private key**, so the easiest path is the official
Kalshi Python SDK pointed at the base URL — it builds the signature for you:

```python
from kalshi_python import KalshiClient   # official SDK

client = KalshiClient(
    base_url="https://kalshi.teddytennant.com/trade-api/v2",
    key_id="ks_live_...",                       # your key id
    private_key_pem=open("you.pem").read(),     # the .pem your admin gave you
)

print(client.get_balance())
print(client.get_markets())
# client.create_order(...) to trade
```

Keep your `.pem` file private — anyone who has it can trade as you.

### Your standings

Your **net worth** is your cash plus any open positions marked to the current price. The
leaderboard at the top of this page combines both platforms and refreshes hourly; the
`predlab` admin tool shows the same standings live.

Made a mess of your account? An admin can reset you back to the starting $25,000 anytime —
just ask.

---

<details>
<summary><strong>For club admins</strong> — running the stack, issuing keys, deploying</summary>

### Repository structure

```
predlab/
├── polymarket-sim/   # Polymarket Gamma + CLOB API mock (Python / FastAPI)   :8001
├── kalshi-sim/       # Kalshi Trade API v2 mock        (Python / FastAPI)    :8002
├── ratatui-admin/    # Admin TUI (Rust): issue dual keys + club roster
├── docker-compose.yml
└── Makefile
```

The two simulators sync live prices from the real public APIs and expose drop-in–compatible
endpoints, so members point the official SDKs at the base URL and only change the host.

### Run the simulators

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab
docker compose up --build      # Polymarket -> :8001, Kalshi -> :8002
```

Or run one directly for development:

```bash
cd polymarket-sim && pip install -e ".[dev]" && uvicorn polymarket_sim.main:app --port 8001
cd kalshi-sim     && pip install -e ".[dev]" && uvicorn kalshi_sim.main:app     --port 8002
```

### Admin TUI

```bash
make install-admin     # cargo install --path ratatui-admin  -> `predlab` on your PATH
predlab                # or: make admin
```

Three tabs. Switch with `l`/`h` (next/prev) or `Tab`/`Shift+Tab` (`h`/`l` type into the
username in the Issue view, so use `Tab` there):

- **Issue keys** — type a username, pick a role with `←/→`, `Enter` mints paper keys on
  *both* simulators, saves the member to the roster, copies a credentials block to your
  clipboard, and writes the Kalshi private key to `~/.predlab/keys/<username>.pem`.
- **Roster** — the club's students from `~/.predlab/students.db` (`j`/`k` or ↑/↓ to select).
  Member ops: `c` re-copies credentials, `r` resets the selected member's balances to the
  starting amount, `R` resets *everyone* (start-of-competition wipe), and `x` permanently
  removes the selected member from both sims and the roster. Destructive actions need a `y`
  confirmation.
- **Leaderboard** — every member ranked by **combined paper net worth** across both sims
  (live; `r` refreshes).

Configure endpoints/secrets via env vars: `POLY_URL`, `KALSHI_URL`, `PREDLAB_ADMIN_SECRET`
(Polymarket admin, `X-Admin-Secret`), and `PREDLAB_KALSHI_SECRET` (Kalshi admin,
`X-Kalshi-Sim-Admin`; falls back to `CLUB_ADMIN_SECRET`).

### Roles

Every user has a role. Key issuance is gated on **both** sims — students cannot self-serve.

| Role     | Can do                                                            |
|----------|-------------------------------------------------------------------|
| `member` | Trade & view **their own** account only (the default).            |
| `admin`  | Issue/revoke keys, reset balances, remove members. (e.g. the VP.) |
| `owner`  | Everything, incl. force-resolving markets and granting roles.     |

The **master secret** (`ADMIN_SECRET` for Polymarket, `CLUB_ADMIN_SECRET` for Kalshi)
authenticates as `owner` — that's your bootstrap/break-glass. An admin/owner can also act
with their **own** key, so you can hand the VP an admin key instead of the master secret.
Only an owner may mint `admin`/`owner` keys. The `predlab` TUI has a role picker (←/→).

### Issuing keys by hand (the TUI does both at once)

```bash
# Polymarket — returns the api_key the member puts in POLY_API_KEY
curl -X POST "https://poly.teddytennant.com/admin/create-paper-key?username=alice&role=member" \
  -H "X-Admin-Secret: $PREDLAB_ADMIN_SECRET"

# Kalshi — returns the RSA private key ONCE (hand the .pem to the member) plus a key id
curl -X POST "https://kalshi.teddytennant.com/trade-api/v2/api_keys/generate?username=alice&role=member" \
  -H "X-Kalshi-Sim-Admin: $CLUB_ADMIN_SECRET" \
  -H "Content-Type: application/json" -d '{"name":"alice-laptop","scopes":["trade"]}'
```

Add `&role=admin` (owner only) to mint an admin key.

### Standings & teaching ops

`GET /admin/leaderboard` (Polymarket) and `GET /trade-api/v2/admin/leaderboard` (Kalshi)
return every member ranked by paper net worth (cash + open positions marked to current
price). Both are admin-gated; the TUI's Leaderboard tab merges them into one ranking.

Resets and removals (admin-gated; the `predlab` TUI fires each on both sims at once):

| Action            | Polymarket                                  | Kalshi                                          |
|-------------------|---------------------------------------------|-------------------------------------------------|
| Reset one member  | `POST /admin/reset-balance?username=alice`  | `POST /trade-api/v2/admin/reset-user?username=alice` |
| Reset **everyone** | `POST /admin/reset-balance` (no username)  | `POST /trade-api/v2/admin/reset-user` (no username)  |
| Remove a member   | `POST /admin/delete-user?username=alice`    | `POST /trade-api/v2/admin/delete-user?username=alice` |

A *reset* is a clean slate — cash returns to the starting balance, open orders are cancelled
and positions cleared, so net worth is exactly the starting amount. *Remove* permanently
deletes the member and all their data (for when someone leaves the club).

### Testing

```bash
make test          # runs all three suites
make test-sims     # pytest for both simulators
make test-admin    # cargo test for the admin tool
```

The simulator tests run fully offline (isolated temp SQLite, no live network sync).

### Deploying for the club

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
- Key issuance is **admin-gated on both sims** — students can't self-serve; only people you
  (or an admin) issue a key to can trade, and only an owner can mint admin/owner keys.
- Members are scoped to their own account on every endpoint.
- CORS is `allow_origins=["*"]`. Harmless for SDK/script clients; tighten if you add a
  browser frontend on a specific origin.

</details>
