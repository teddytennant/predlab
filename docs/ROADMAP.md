# PredLab — Club-Launch Roadmap

What's left to build before PredLab is a smooth, self-running school club. Derived
from a fan-out gap analysis (2026-06-08) and ordered by priority. Items already
shipped this pass are checked off at the bottom.

Priority: **must** (blocks a good launch) · **should** (real quality-of-life) · **nice** (later).

## Must — do before/at launch

- [ ] **Auto-resolve closed markets** *(correctness, medium)* — closed markets never
  settle automatically; an owner must `force-resolve` each one by ID. With hundreds
  of synced markets, winners go unpaid and net worth (the whole leaderboard) drifts
  wrong. Read the resolved outcome from Gamma in `sync.py` (`umaResolutionStatus` /
  terminal `outcomePrices`) and call `force_resolve_market` with the winning leg when
  a market flips closed. Keep manual force-resolve as an override. *(The dead
  `maybe_auto_settle_on_sync` stub was removed — this is its real replacement.)*

- [ ] **Seed-liquidity bot** *(engagement, medium)* — there's no house market-maker,
  so an empty book means the first orders at the first meeting sit `open` forever and
  students lose interest in week one. Run a club account (excluded from the board via
  `EXCLUDE_USERS`) that rests small bids/asks around each market's Gamma midpoint
  (`/midpoint` + `/order`) so orders near the spread fill within seconds.

- [ ] **Bulk key issuance** *(onboarding, medium)* — issuing keys is one-at-a-time;
  signing up a whole club at a fair is a slow loop. Add an admin action that takes a
  pasted list/CSV of usernames and mints keys for all, returning a printable
  username+key table (optionally a per-student claim card / QR to `/start`).

- [ ] **Nightly Postgres backups** *(ops, small)* — a host failure wipes a semester of
  standings. Add a systemd timer (or sidecar) running `pg_dump` to a retained/off-box
  location nightly. Manual command is in OPERATIONS.md meanwhile.

- [x] **Public "about / rules" page** — shipped: `/about` on the leaderboard site.
- [x] **Operations runbook** — shipped: `docs/OPERATIONS.md`.

## Should — strong quality-of-life

- [ ] **Market curation / "this week's markets"** *(medium)* — members face the entire
  synced catalog with no shared focus. Add an admin-settable featured flag + a
  `/markets?featured=1` filter, and a "this week" section on the site and TUI.

- [ ] **Competition rounds + archived results** *(large)* — the only fresh start is
  `reset-balance` (no username), which destroys history. Add a round model (name +
  start/end), freeze final standings into an archive table at end, then reset; show a
  "Past competitions / Hall of Fame" page. (Manual JSON archive in OPERATIONS.md for now.)

- [ ] **Engagement/activity view for admins** *(medium)* — the board ranks by net worth
  only; admins can't see who's never traded or gone quiet. Add `/admin/activity` (or a
  TUI tab) with per-member last-trade time + trade count from `/data/trades`, flagging
  zero-trade / 7-day-idle members.

- [ ] **Fill / resolution notifications** *(medium)* — members must poll to learn
  anything. Add a Telegram-bot or email ping on fill/resolution (channel plan already
  exists in `docs/telegram/`), or a per-member events feed the TUI toasts.

- [ ] **Sync-staleness monitoring** *(medium)* — if the Gamma sync loop stalls, prices
  silently go stale and nobody's alerted. Expose last-sync time + market count in
  `/health`, watch `/healthz`, and show a "prices as of HH:MM" indicator on the board.

## Nice — later

- [ ] **Weekly auto-recap** to the Telegram channel (diff net-worth snapshots
  week-over-week; post top gainer / biggest mover / current #1).
- [ ] **Guided first trade** — a `predlab.py demo()` that buys a tiny position in a
  known-liquid featured market (fills against seed liquidity) and prints the result.
- [ ] **Published API reference** for agent-builders — link the FastAPI `/docs` from
  the site and add a concise endpoint table to the README's AI-toolset section.
- [ ] **Biggest-movers panel / per-member sparklines** on the site — both need
  historical net worth per member in one shot; add a small sim endpoint
  (`/admin/movers` or standings-with-spark from the existing `NetWorthSnapshot` table)
  rather than N per-user calls per render.

## Open correctness item (from the latest review)

- [ ] **Market orders should be IOC** *(medium, mints money)* — an unfilled market
  order rests in the book at the sentinel price 999 (buy) / 0 (sell) instead of being
  cancelled; a later cross can fill at 999/share, creating cash. Make market orders
  cancel/refund any unfilled remainder after matching rather than resting the
  sentinel. See `services/paper_trading.py` order placement + `orderbook.py`
  `add_limit_order`. (Distinct from the three HIGH bugs already fixed.)

## Shipped this pass (2026-06-08)

Website made more featureful + slop cleanup:
- `/about` rules page, `/markets` browser (search + paging), club-stats block, **P&L**
  column, `/api/user/:username` JSON profile.
- Fixed: username→profile link collision (numeric/duplicate names), excluded accounts
  leaking via `/u/`, `/leaderboard.json` 200-on-outage (now 502), startup warning when
  the admin secret is unset.
- Removed dead code (`get_available_balance`, `maybe_auto_settle_on_sync`, admin
  `leaderboard.rs` module, dead batch-order lines, dead compose env var); corrected
  misleading "stub" labels, the net-worth formula in README/overview, and the
  `POLY_KEY`→`POLY_API_KEY` client mismatch; added `cancel_order()` to the client and
  a deterministic tie-break + `error_for_status` to the admin TUI.
