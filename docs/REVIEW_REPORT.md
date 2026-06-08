# PredLab — Full Review Report

**Date:** 2026-06-08 · **Branch:** `main` @ `c425aff` · **Method:** four parallel review
agents (secrets/auth, endpoint consistency, Rust services, Python sim/client) plus manual
verification of the top findings.

## TL;DR

The repo is **legitimate and in good shape**. No leaked secrets, clean git history, the
external Polymarket integration is the genuine public read-only Gamma API with no
credentials sent, every documented endpoint exists, and all clients call paths/headers/URLs
that match the server. The Rust workspace builds clean with zero clippy warnings; the Python
suite is **54/54 passing**.

The real issues are a small number of **paper-accounting money-conservation bugs** and a
**deployment-time auth default** that must be overridden in production. None of them are
secret leaks, and none affect the legitimacy of the API/endpoints — they're correctness and
hardening items. Priority order is below.

> **Update (2026-06-08):** all three 🔴 HIGH findings are now **fixed** (commit on `main`)
> with regression tests — suite is now **61/61 passing**. See the ✅ notes on each below; the
> 🟠/🟡 items remain open.

## Verdict on the explicit asks

| Question | Answer |
|---|---|
| **Are any real API keys / secrets leaked?** | **No.** `.gitignore` excludes `.env`; no `.env` ever existed in git history; all `.env.example` values are placeholders. No secrets baked into any Rust binary or the Python code — all read from env vars. |
| **Are the endpoints legitimate / consistent?** | **Yes.** Every README-documented endpoint exists in `main.py` with matching path+method. All clients (predlab.py, predlab-tui, ratatui-admin, leaderboard-rs) use the correct base URLs and the `POLY_API_KEY` / `X-Admin-Secret` headers. |
| **Is the external integration legitimate?** | **Yes.** Only external host contacted is `gamma-api.polymarket.com` (the real public Polymarket Gamma API), read-only GET, no key sent, only a benign `User-Agent`. No `clob.polymarket.com` calls; CLOB-shaped local endpoints are served from the internal paper book. |

## Findings

Severity: 🔴 high · 🟠 medium · 🟡 low · ⚪ info. Every item cites `file:line`.

### 🔴 HIGH — Default admin secret grants `owner` if env unset
`polymarket-sim/src/polymarket_sim/config.py:47` →
`admin_secret = "change-me-in-prod-for-club"`, compared at `services/auth.py:155`.
If a deployment doesn't set `ADMIN_SECRET`, this repo-public string becomes a live **owner**
credential: anyone sending `X-Admin-Secret: change-me-in-prod-for-club` can force-resolve
markets (decide winners), reset balances, mint owner keys, and delete users.
`docker-compose.yml:30,53` reinforces this with `${ADMIN_SECRET:-change-me-set-in-dotenv}`.
**Fix:** refuse to boot (or disable secret-based owner auth) when `admin_secret` is empty or
equals a known placeholder and `environment` is production-like. *Confirmed by two
independent agents + manual read.*
**✅ FIXED:** `config.py` now has a `model_validator` that raises on a placeholder/empty
`admin_secret` whenever `environment` isn't dev/local. Dev still boots on the default.
Tests: `tests/test_config.py`.

### 🔴 HIGH — Naked short sell creates money from nothing
`paper_trading.py:249-253` (sell branch) + `:165-177` (`update_position_on_fill`).
A sell with insufficient position is "demo allowed" (warning only). On fill the user is
credited `fill_price*size` cash, but the position is clamped to `max(0.0, …)` instead of
going negative. A user with zero shares can sell into a resting bid, pocket cash, and keep a
0 position — pure money creation that inflates leaderboard net worth.
**Fix:** reject sells exceeding current position size (mirror the buy-side balance check), or
track and value negative (short) positions in `compute_net_worth`.
**✅ FIXED (short-tracking option):** rejecting sells would break the engine's maker-seeding
model (makers rest naked asks to create a two-sided book — encoded in the test suite). Instead
`update_position_on_fill` now lets the position go **negative**; the short is marked as a
liability by `compute_net_worth` (`size * mark`), so the sale proceeds are offset and net worth
no longer inflates. Test: `test_naked_sell_opens_short_and_does_not_mint_money`.

### 🔴 HIGH — Cancel leaves the resting entry in the book → phantom fills / double credit
`paper_trading.py:427-447` (`cancel_paper_order`, "removal from live book omitted for MVP").
A single-order cancel refunds the buy escrow and marks `status="cancelled"` but never removes
the `OrderBookEntry` from the global in-memory book. A later incoming order still matches the
cancelled entry; `_settle_resting_counterparty` reloads it, processes the fill, and credits
proceeds — **after** it was already refunded. Maker gets refund *plus* fill proceeds.
**Fix:** remove the entry inside `cancel_paper_order`, and guard `_settle_resting_counterparty`
to skip makers whose status is `cancelled`.
**✅ FIXED:** added `remove_resting_order(order_id)` to `orderbook.py`, called from
`cancel_paper_order`; plus a defensive `status == "cancelled"` guard in
`_settle_resting_counterparty`. Test: `test_cancelled_order_is_purged_from_book_and_cannot_refill`.

### 🟠 MEDIUM — Bearer API keys stored plaintext; hashed secret never validated
`services/auth.py:60-96`. Auth is a plain DB lookup on `ApiKey.key_prefix == api_key`
(`:73`); for `pm_paper_` keys the secret/signature check is skipped (`:79-82`) and
`secret_hash` is never verified anywhere. The effective credential is the `key_prefix`,
stored **plaintext** (`:243-248`) — a DB read yields directly usable, full-power keys; the
hashed-secret design adds no auth value as wired. Online brute-force risk is negligible
(~96-bit prefix) and it's fake money, but at-rest exposure is real.
**Fix:** either verify the high-entropy secret against its stored hash (look up by prefix,
`compare_digest` the hash), or at minimum store/compare a hash of the prefix.

### 🟠 MEDIUM — Market buy can walk past escrow and partially commit on failure
`paper_trading.py:240-248, 305-314` + `db.py:99-109`. A market buy validates only
`mark*size` at placement but matches at `price=999`, so it can fill higher up the book; when
a reconcile pushes the balance below 0, `_adjust_balance` raises mid-loop. The route handler
(`main.py:454`) catches it and returns 200, so `get_session` commits the **partially**
applied state (earlier fills/positions/balance deltas; `order.filled_size` never updated).
**Fix:** validate worst-case fill cost before matching, or `session.rollback()` / re-raise on
failure so the session rolls back.

### 🟠 MEDIUM — Duplicate `POST /orders` route; second handler is dead, unauthenticated code
`main.py:460` (`post_batch_orders`, auth-required) and `main.py:765` (`legacy_create_order`,
**no auth**) both register `POST /orders`. FastAPI matches the first, so the legacy no-auth
handler is unreachable today — but it's a latent bypass if the routes are ever reordered, plus
dead lines at `main.py:491-492`.
**Fix:** delete the legacy handler and the dead `continue` block.

### 🟠 MEDIUM — O(N markets) scans on every price/token lookup
`paper_trading.py:40-46` (`_get_market_by_token`) and `main.py:507-512`
(`_find_market_for_token`) load and scan all markets per call. `compute_net_worth` runs this
per position, and the leaderboard runs `compute_net_worth` per user → O(users × positions ×
markets), with `sync_max_markets` up to thousands.
**Fix:** index by `market_id` (already on `Position`/`Order`) or build a token→market map once
per request.

### 🟠 MEDIUM (leaderboard-rs) — Truncated usernames can collide and mislink profiles
`leaderboard-rs/src/main.rs:528-562`. Usernames are truncated to 28 chars then linked via
`html.replacen(escaped, link, 1)`; truncation breaks the "still unique" assumption, so two
members sharing the first 27 chars get a nested/incorrect anchor pointing at the wrong
profile.
**Fix:** insert per-row link placeholders by row index instead of string-replacing the
rendered table, or key the replace on the full untruncated username.

### 🟡 LOW — `/leaderboard.json` returns `[]` with 200 on upstream failure
`leaderboard-rs/src/main.rs:200-211`: `fetch_leaders(...).unwrap_or_default()` collapses a
sim outage into an empty array, so `predlab-tui` can't tell "no members" from "sim down."
**Fix:** return a non-200 / error envelope.

### 🟡 LOW — Latent unauthenticated `/orders` (same as the MEDIUM dup, security framing)
Already covered above — the dead legacy route would place orders as `demo_trader`/user 1 with
no auth if ever un-shadowed (`main.py:765-799`).

### 🟡 LOW — CORS wildcard with credentials
`main.py:221-227`: `allow_origins=["*"]` + `allow_credentials=True` + `*` methods/headers.
Over-permissive (and an invalid combination for credentialed browser requests). Auth is via
custom headers not cookies, so impact is reduced, but tighten to an allow-list before public
exposure. The code comment already says "dev only — tighten for production."

### 🟡 LOW — Internal error text leaked to clients
`main.py:457` returns `str(exc)[:200]`; `main.py:531-532` raises `HTTPException(400, str(exc))`.
Surfaces accounting internals. Low risk on a paper sim.

### 🟡 LOW — Rust hardening nits
- Non-deterministic tie-break in live leaderboard sorts (`leaderboard-rs/src/main.rs:247-251`,
  `ratatui-admin/src/main.rs:419-423`) — equal net worths can flicker order; the alphabetical
  tie-break helper (`ratatui-admin/src/leaderboard.rs:14`) is dead code. Reuse it.
- `ratatui-admin` `issue_poly` (`main.rs:503-520`) skips `error_for_status()` before parsing,
  hiding the real HTTP error (bad secret / role) behind "response missing api_key."
- `reqwest::Client` rebuilt per request in the admin TUI (`main.rs:398,430,504`) — no pool
  reuse. Minor.
- `fmt_money`/`truncate` logic triplicated (`predlab-util/src/lib.rs:7`,
  `leaderboard-rs/src/main.rs:256,295`, `ratatui-admin/src/leaderboard.rs:25`). Deliberate
  (leaderboard-rs builds in an isolated Docker context) but a future fix must touch 3 places.

### 🟡 LOW — Client/docs nits
- `examples/predlab.py`: `markets()`/`book()` (`:38,42`) skip `raise_for_status()` while
  `positions()`/`place_order()` use it; no client-side validation that `price∈[0,1]`,
  `size>0`; an unrecognized `side` is silently coerced to "buy" server-side
  (`main.py:385-392`). Add a guard.
- The README prose (line 120) implies a `/balance` endpoint, but **no `/balance` route
  exists** — cash is the `cash` field of `GET /portfolio`. Fix the wording.
- `predlab.py` docstring/smoke test reads env var `POLY_KEY` (`:9,62`) while everything else
  (and the actual header) uses `POLY_API_KEY`. Align to avoid member confusion.
- Several real endpoints are undocumented: `/events`, `/books`, `DELETE /order`,
  `/user/orders`, `/data/orders`, `/data/trades`, `/admin/set-role`, `/admin/revoke-key`,
  `/admin/force-resolve`, `/admin/user/{username}`.

## What's solid (don't touch)

- **No leaked secrets.** `.gitignore` correct; `git log --all --full-history -- .env` empty;
  all binaries read secrets from env only.
- **CSPRNG key generation** — `secrets.token_urlsafe` (~96-bit prefix + 256-bit secret),
  `auth.py:47-57`; timing-safe admin compare (`compare_digest`, `auth.py:155`).
- **Role gating + member scoping** verified across every admin and member route. `DEV_BYPASS_AUTH`
  confirmed fully removed.
- **External integration is clean** — public Gamma API only, read-only, no credentials.
- **All documented endpoints exist; all clients match** paths/headers/base URLs.
- **No SQL injection** — SQLAlchemy ORM / bound params throughout; the one raw statement is a
  static `ALTER TABLE` with no interpolation (`db.py:78-81`).
- **Tests green:** Python **61/61** (`pytest`, incl. the 3 new HIGH-fix regressions; 54/54
  at audit time), Rust workspace + leaderboard-rs build clean, `cargo clippy --all-targets`
  zero warnings, `cargo test` all pass.

## Recommended fix order

1. **Override `ADMIN_SECRET` in the live `.env` right now** (operational), then add the
   boot-time guard (🔴 config.py:47).
2. Fix the two money-conservation bugs (🔴 naked short sell, 🔴 cancel-not-removed-from-book).
3. Delete the dead duplicate `/orders` route (🟠 main.py:765).
4. Decide on key-at-rest handling (🟠 plaintext keys) and market-buy rollback (🟠).
5. Sweep the LOW docs/Rust nits.

## Concurrency note

The global in-memory book and balance read-modify-write have no locking; they're safe **only**
because the route handlers call the fully-synchronous `place_paper_order` with no `await`
inside, so the event loop can't interleave placements. Add a lock/comment before ever making
those handlers truly async or threadpool-offloaded.
