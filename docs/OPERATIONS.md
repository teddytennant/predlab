# PredLab — Operations Runbook

A step-by-step playbook for running the club week to week. Written so a co-officer
(VP / admin) can run a meeting without the founder present. Everything here uses
endpoints and TUI actions that already exist.

> Admin actions need the master secret (`X-Admin-Secret: $ADMIN_SECRET`) or an
> `owner`/`admin` API key. The admin TUI (`ratatui-admin`) wraps the common ones.

## The stack (where things run)

| Piece | What | Health check |
|---|---|---|
| `postgres` | members, balances, trades, history | `docker compose ps` |
| `polymarket-sim` (`:8001`) | API + paper accounting + Gamma sync | `GET /health` |
| `leaderboard-rs` (`:8003`) | public site (`predlab.teddytennant.com`) | `GET /healthz` |

Exposed via Cloudflare Tunnel — no open inbound ports.

## Before every meeting (5 min)

1. **Stack up?** `docker compose ps` — all services `Up`. If not: `docker compose up -d`.
2. **Prices fresh?** `curl -s localhost:8001/health` and open `predlab.teddytennant.com`
   — the board should render and net worths look sane (nobody at exactly $25,000.00
   across the board, which would mean sync/marks are stale).
3. **Markets loaded?** `curl -s "localhost:8001/markets?limit=3"` returns questions.
   The public markets browser is at `predlab.teddytennant.com/markets`.

## Onboarding new members

Issue one key per member (admin TUI → **Roster** → add, or the API):

```bash
curl -X POST "localhost:8001/admin/create-paper-key?username=alice&display_name=alice&role=member" \
  -H "X-Admin-Secret: $ADMIN_SECRET"
```

Hand each member their `pm_paper_…` key (the TUI's `c` copies a creds block) and
point them at `predlab.teddytennant.com/start`. Every member starts with $25,000.

> **Onboarding 30+ at once is still one-at-a-time today** — see ROADMAP "bulk key
> issuance". For now, pre-mint keys before the fair and print the creds blocks.

## Running a weekly competition

1. **(Optional) reset for a clean round.** `POST /admin/reset-balance` with *no*
   username resets **everyone** to $25,000 and clears positions/orders. Per-member:
   add `?username=alice`. **This wipes history — archive standings first** (below).
2. **Feature the week's markets.** Tell members which questions you're trading this
   week (there's no in-app "featured" flag yet — see ROADMAP). Link them from the
   markets browser by question.
3. **Members trade** via `predlab.py` (the `predlab-py` package), the desktop GUI, or their own agents.
4. **Watch the board** live at `/` (refreshes every 30s) — the P&L column and club
   stats show how the cohort is doing.

## Resolving markets (paying out winners)

Closed markets do **not** auto-settle yet (see ROADMAP — top correctness item).
Resolve each market you featured at end of round:

```bash
# resolution is the winning leg: "yes" or "no"
curl -X POST "localhost:8001/admin/force-resolve?market_id=<ID>&resolution=yes" \
  -H "X-Admin-Secret: $ADMIN_SECRET"   # owner-only
```

Winners' YES (or NO) shares pay $1.00, losers $0. Net worth updates immediately.

## Archiving a round (before a reset)

There's no automatic archive yet. To preserve a round's final standings, snapshot
the leaderboard JSON before resetting:

```bash
curl -s localhost:8001/admin/leaderboard -H "X-Admin-Secret: $ADMIN_SECRET" \
  > standings-$(date +%F).json
```

Keep these somewhere durable; they're your "hall of fame" record until the archive
feature lands.

## Backups (do this!)

A host/disk failure currently wipes a whole semester of standings. Until the
automated nightly dump lands (ROADMAP), run a manual dump regularly:

```bash
docker compose exec -T postgres pg_dump -U predlab polymarket > predlab-$(date +%F).sql
```

Restore: `docker compose exec -T postgres psql -U predlab polymarket < predlab-DATE.sql`.

## When something's wrong

| Symptom | Likely cause | Fix |
|---|---|---|
| Board shows "standings temporarily unavailable" | `PREDLAB_ADMIN_SECRET` unset/wrong, or sim down | check leaderboard env + `docker compose logs polymarket-sim` |
| All net worths frozen / equal | Gamma sync stalled | restart sim: `docker compose restart polymarket-sim` |
| Member gets `401` | bad/missing `POLY_API_KEY`, or key revoked | re-issue a key |
| Order stuck `open` forever | thin book, nobody crossing | member can cancel (DELETE `/order`) or move price toward best bid/ask |
| Page is the error placeholder but sim is up | leaderboard can't reach sim over docker net | check `POLY_URL` in leaderboard env |

## Quick reference — admin endpoints

| Do | Call |
|---|---|
| Issue key | `POST /admin/create-paper-key?username=&display_name=&role=` |
| Revoke key | `POST /admin/revoke-key?…` |
| Set role | `POST /admin/set-role?…` |
| Reset one | `POST /admin/reset-balance?username=alice` |
| Reset all | `POST /admin/reset-balance` |
| Remove member | `POST /admin/delete-user?username=alice` |
| Force-resolve | `POST /admin/force-resolve?market_id=&resolution=` (owner) |
| Standings | `GET /admin/leaderboard` |
| Member detail | `GET /admin/user/{username}` |
