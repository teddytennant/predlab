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

Before exposing this publicly: set real admin secrets (`ADMIN_SECRET`,
`CLUB_ADMIN_SECRET`), turn off the Kalshi auth bypass (`DEV_BYPASS_AUTH=false`), and tighten
CORS. A Cloudflare Tunnel from the host plus a `systemd` service per simulator is the
simplest way to put `:8001`/`:8002` behind your domain with HTTPS.
