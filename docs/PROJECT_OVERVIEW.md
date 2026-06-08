# PredLab — Project Overview

**PredLab** is the paper-trading playground for the **NCSSM Prediction Markets Club**.
Members trade Polymarket-style yes/no markets with $25,000 of fake money, practice real
strategies, and climb a live club leaderboard. There is nothing to install on the server
side for a member — an admin issues a key and the member trades against the club's hosted
API.

> **Paper trading only. Not affiliated with Polymarket. Educational use, fake money.**

- **Leaderboard:** https://predlab.teddytennant.com
- **API:** https://poly.teddytennant.com
- **Source:** https://github.com/teddytennant/predlab

## What it is

A market asks a yes/no question ("Will X happen?"). You buy **YES** or **NO** shares
priced $0.01–$0.99 — the price *is* the market's implied probability. A winning share pays
$1.00 at resolution; a losing share pays $0. Your **net worth = free cash + positions
marked at the current price + cash escrowed in resting buy orders**, and that's your
leaderboard score.

The simulator pulls live prices for the real, liquid Polymarket catalog from the public
**Gamma API** (`gamma-api.polymarket.com`, read-only, no key) and exposes drop-in-style
CLOB endpoints backed by a club-internal paper order book. There is **no house
market-maker** — your order fills only when another member takes the other side.

## Architecture

| Component | Language / stack | Port | Role |
|---|---|---|---|
| `polymarket-sim/` | Python · FastAPI · SQLAlchemy | 8001 | Market sync, order book, paper accounting, admin + auth |
| `leaderboard-rs/` | Rust · axum | 8003 | Public live leaderboard site + per-member profile pages |
| `ratatui-admin/` | Rust · ratatui (TUI) | — | Admin console: issue/revoke keys, manage roster, reset balances |
| `predlab-tui/` | Rust · ratatui (TUI) | — | Member TUI: leaderboard + markets + portfolio in one window |
| `predlab-util/` | Rust (lib) | — | Shared formatting/util crate for the two TUIs |
| `examples/predlab.py` | Python (single file) | — | Member starter client (`pip install requests`) |

Deployment is a `docker compose` stack (Postgres + sim + leaderboard) on a NixOS host,
exposed through a **Cloudflare Tunnel** — no open inbound ports.

## Roles & access model

| Role | Can do |
|---|---|
| `member` | Trade & view **their own** account only (default). |
| `admin` | Issue/revoke keys, reset balances, remove members. |
| `owner` | Everything, incl. resolving markets and granting roles. |

- Key issuance is admin/owner-gated — members cannot self-serve.
- Members are scoped to their own account (`user.id`) on every authenticated endpoint.
- The master `ADMIN_SECRET` (sent as `X-Admin-Secret`) authenticates as `owner`.
- Member API keys look like `pm_paper_xxxxxxxx` and are sent in the `POLY_API_KEY` header.

## The AI angle

The club's stance: **AI use is unrestricted and encouraged.** The PredLab API is designed
as an agent toolset (browse markets, read a book, check a portfolio, place/cancel orders).
The founder runs a reference agent — **Hermes on xAI's Grok** — that trades autonomously on
the leaderboard. Members are encouraged to bring their own models (Claude, GPT, Llama),
their own frameworks, or no AI at all.

## Repository map

```
predlab/
├── polymarket-sim/   # FastAPI Gamma + CLOB mock + paper accounting   :8001
├── leaderboard-rs/   # Public live leaderboard (axum)                  :8003
├── ratatui-admin/    # Admin TUI (issue keys, manage roster)
├── predlab-tui/      # Member TUI (leaderboard + markets + portfolio)
├── predlab-util/     # Shared Rust util crate
├── examples/         # Member starter client (predlab.py)
├── docs/             # This overview, review report, club docs, Telegram plan
├── docker-compose.yml
└── Makefile
```

See [`REVIEW_REPORT.md`](REVIEW_REPORT.md) for the latest full security/correctness audit,
[`OPERATIONS.md`](OPERATIONS.md) for the weekly meeting runbook, [`ROADMAP.md`](ROADMAP.md)
for what's left before launch, and [`telegram/`](telegram/) for the community channel plan
and ready-to-post content.
