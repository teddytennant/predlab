# PredLab

**PredLab** is the paper-trading playground for the school Prediction Markets Club.

Trade on Polymarket-style markets with fake money, practice real strategies, and climb the club leaderboard. Nothing to install — your admin gives you a key and you trade against the club's hosted server.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET.** Educational use, fake money.

## 🏆 Leaderboard

**[predlab.teddytennant.com](https://predlab.teddytennant.com)** — live paper net-worth standings, updated automatically.

## Getting started (members)

You don't run or install any servers. Ask a club admin for two things:

- your **username**, and
- your **API key** — a string like `pm_paper_xxxxxxxx`. You send it as the `POLY_API_KEY` header on every request that touches your account.

Everyone starts with **$25,000 of paper money** and trades against the club's hosted server at **`https://poly.teddytennant.com`**. It's fake money — experiment freely.

> **New to prediction markets?** A market asks a yes/no question ("Will X happen?"). You buy **YES** or **NO** shares priced between **$0.01 and $0.99** — the price *is* the market's estimated probability. Each winning share pays out **$1.00** when the market resolves; losers pay $0. Buy low on an outcome you believe in, and you profit if you're right.

### Step 1 — Get the client

The entire client is a single file: [`examples/predlab.py`](examples/predlab.py). Download it and install its one dependency:

```bash
pip install requests
# save examples/predlab.py in the folder you'll work from
```

(No Python? Skip to the [curl section](#prefer-curl-or-another-language) — any HTTP tool works.)

> **Prefer a terminal?** [`predlab-tui`](predlab-tui/) is a vim-flavored TUI clone of the leaderboard site: standings, markets, and your portfolio in one window. Install with `cargo install --git https://github.com/teddytennant/predlab predlab-tui`, then `export POLY_API_KEY=…` and run `predlab-tui`. The install page (with screenshots and key reference) is also linked from [predlab.teddytennant.com/tui](https://predlab.teddytennant.com/tui).

### Step 2 — Plug in your key

In a Python shell or script **in the same folder** as `predlab.py`:

```python
from predlab import PolymarketClient

poly = PolymarketClient(api_key="pm_paper_REPLACE_ME")   # the key your admin gave you
```

`poly` now talks to the club server. (To point at a local sim instead, set the `POLY_BASE` environment variable.)

### Step 3 — Browse markets and grab a token

You trade **outcome tokens**, not markets. Every market has a `clobTokenIds` list: index **`0` = YES**, index **`1` = NO**. You need one of those token ids to place an order.

```python
markets = poly.markets(limit=5)        # browsing needs no key
m = markets[0]
print(m["question"])                   # "Will ... ?"
print(m["bestBid"], m["bestAsk"])      # current market price (0–1)

yes_token = m["clobTokenIds"][0]       # the YES outcome token
no_token  = m["clobTokenIds"][1]       # the NO outcome token
```

### Step 4 — Place an order

`price` is the per-share cost between **0.01 and 0.99**; `size` is the number of shares.

```python
# "YES looks underpriced — buy 10 shares at 55¢"
resp = poly.place_order(token_id=yes_token, side="BUY", price=0.55, size=10)
print(resp)   # {'success': True, 'orderID': '...', 'status': 'open' | 'filled' | 'partial'}
```

Check the `status` it returns:

- **`filled`** — matched immediately; you now hold the shares.
- **`open`** — your order is resting on the book, waiting for another member to take the other side.
- **`partial`** — part filled, the rest is resting.

> **Why might my order sit `open`?** The order book is shared by the whole club and there's **no house market-maker** — your order only fills when *another member* trades against it. Early in a competition the book can be thin, so orders may wait. To fill faster, **buy near `bestAsk`** (or **sell near `bestBid`**) to cross the spread.

To sell shares you hold, pass `side="SELL"`.

### Step 5 — Check your account

```python
print(poly.positions())   # the shares you hold + unrealized profit/loss
```

For your cash and total net worth (the number that ranks you):

```bash
curl https://poly.teddytennant.com/portfolio -H "POLY_API_KEY: pm_paper_REPLACE_ME"
# -> {"cash": 24994.5, "positions_value": 5.5, "open_orders_value": 0.0, "net_worth": 25000.0}
```

**Net worth = free cash + your positions marked at the current market price + cash escrowed in your resting buy orders.** That's your leaderboard score.

### Step 6 — Climb the leaderboard

Watch your standing at **[predlab.teddytennant.com](https://predlab.teddytennant.com)** — it refreshes automatically.

### Prefer curl or another language?

Any HTTP client works. Send your key in the `POLY_API_KEY` header (`Authorization: Bearer <key>` also works):

```bash
# Browse markets — no key needed
curl "https://poly.teddytennant.com/markets?limit=5"

# Place an order — needs your key (token_id comes from a market's clobTokenIds)
curl -X POST https://poly.teddytennant.com/order \
  -H "POLY_API_KEY: pm_paper_REPLACE_ME" -H "Content-Type: application/json" \
  -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'

# Your positions and portfolio
curl https://poly.teddytennant.com/positions -H "POLY_API_KEY: pm_paper_REPLACE_ME"
curl https://poly.teddytennant.com/portfolio -H "POLY_API_KEY: pm_paper_REPLACE_ME"
```

**Key needed** for anything touching your account (orders, positions, portfolio) — without it you get `401`. **No key** for public market data (`/markets`, `/book`, `/midpoint`, `/spread`, `/last-trade-price`).

### Troubleshooting

| Symptom | What it means / fix |
|---|---|
| `401 Unauthorized` | Key missing, mistyped, or revoked. Re-check the `POLY_API_KEY` header or ask your admin. |
| Order stays `open`, never fills | No one has taken the other side yet. Move your price toward `bestBid`/`bestAsk`, or wait for club activity. |
| `unknown token` error | You passed a market `id` — use a value from that market's `clobTokenIds` instead. |
| Wrecked your balance | Ask an admin to reset you to the starting **$25,000**. |

---

<details>
<summary><strong>For club admins</strong> — running the stack, issuing keys, deploying</summary>

### Repository structure

```
predlab/
├── polymarket-sim/   # Polymarket-style Gamma + CLOB mock (Python / FastAPI)  :8001
├── leaderboard-rs/   # Public live leaderboard page (Rust / axum)             :8003
├── ratatui-admin/    # Admin TUI (Rust): issue keys + manage club roster
├── predlab-tui/      # Member TUI (Rust): vim leaderboard + markets + portfolio
├── examples/         # Member starter client (predlab.py)
├── docker-compose.yml
└── Makefile
```

The simulator syncs live prices from the real public Polymarket APIs and exposes drop-in–compatible endpoints.

### Run the simulator

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab
docker compose up --build      # Polymarket -> :8001, leaderboard -> :8003
```

Or run directly for development:

```bash
cd polymarket-sim && pip install -e ".[dev]" && uvicorn polymarket_sim.main:app --port 8001
```

### Admin TUI

```bash
make install-admin     # cargo install --path ratatui-admin  -> `predlab` on PATH
predlab                # or: make admin
```

Three tabs (`l`/`h` or Tab to switch):

- **Issue key** — type a username, pick a role with `←/→`, `Enter` mints a paper API key on the sim, saves the member to the local roster (`~/.predlab/students.db`), and copies the credentials block to your clipboard.
- **Roster** — browse members. `c` copies creds, `r` resets selected balance, `R` resets everyone, `x` removes member (destructive actions require `y` confirm).
- **Leaderboard** — live ranking by paper net worth (`r` to refresh).

Env vars: `POLY_URL` (default http://localhost:8001), `PREDLAB_ADMIN_SECRET` (for `X-Admin-Secret`).

### Roles

| Role     | Can do                                                            |
|----------|-------------------------------------------------------------------|
| `member` | Trade & view **their own** account only (default).                |
| `admin`  | Issue/revoke keys, reset balances, remove members. (e.g. the VP.) |
| `owner`  | Everything, incl. resolving markets and granting roles.           |

The **master secret** (`ADMIN_SECRET`) authenticates as `owner` — your bootstrap key. Only owners can mint `admin`/`owner` keys.

### Issuing keys (curl)

```bash
# Returns the api_key the member uses as POLY_API_KEY
curl -X POST "https://poly.teddytennant.com/admin/create-paper-key?username=alice&role=member" \
  -H "X-Admin-Secret: $PREDLAB_ADMIN_SECRET"
```

Add `&role=admin` (owner only) for admin keys.

### Admin operations

`GET /admin/leaderboard` (admin-gated) returns members ranked by net worth.

Resets / deletes (admin-gated):

| Action            | Polymarket endpoint                          |
|-------------------|----------------------------------------------|
| Reset one member  | `POST /admin/reset-balance?username=alice`   |
| Reset everyone    | `POST /admin/reset-balance`                  |
| Remove a member   | `POST /admin/delete-user?username=alice`     |

A reset clears orders/positions and returns cash to the starting balance. Remove is permanent.

### Testing

```bash
make test          # all suites
make test-sims     # pytest for the simulator
make test-admin    # cargo test for the TUI + registry
make lint
```

Simulator tests are fully offline (temp SQLite, no network).

### Deploying

The live instance is a `docker compose` stack (Postgres + polymarket-sim + leaderboard-rs) on a NixOS host, exposed via Cloudflare Tunnel (no open inbound ports).

Production config in a gitignored `.env`:

```bash
cp .env.example .env   # set a strong ADMIN_SECRET
docker compose up -d --build
```

Update with `git pull --ff-only && docker compose up -d --build`.

**Access model:**
- Key issuance is admin/owner-gated — students cannot self-serve.
- Members are scoped to their own account.
- CORS is open (`*`) for convenience with SDKs/scripts.

</details>
