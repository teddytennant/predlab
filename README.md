# PredLab

**PredLab** is the paper-trading playground for the school Prediction Markets Club.

Trade on Polymarket-style markets with fake money, practice real strategies, and climb the club leaderboard. Nothing to install — your admin gives you a key and you trade against the club's hosted server.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET.** Educational use, fake money.

## 🏆 Leaderboard

**[predlab.teddytennant.com](https://predlab.teddytennant.com)** — live paper net-worth standings, updated automatically.

## Getting started

There are no servers for you to run. A club admin sets you up with:

- a **username**, and
- a **Polymarket API key** (a string you send as the `POLY_API_KEY` header).

Everyone starts with **$25,000 of paper money**, traded against the club's hosted server at `https://poly.teddytennant.com`.

### Trading in 3 steps — download one file

The whole client is a single file, [`examples/predlab.py`](examples/predlab.py). It talks to the Polymarket-style sim so you don't need any SDK.

1. **Download** [`examples/predlab.py`](examples/predlab.py) and `pip install requests`.
2. **Paste in your key** and trade:

   ```python
   from predlab import PolymarketClient

   poly = PolymarketClient(api_key="pm_paper_...")           # your key
   print(poly.markets(limit=5))                              # browse markets
   poly.place_order(token_id="<token>", side="BUY", price=0.55, size=10)
   print(poly.positions())
   ```

3. **Climb the [leaderboard](https://predlab.teddytennant.com).** Your net worth updates automatically.

Full walkthrough: [`examples/README.md`](examples/README.md). Prefer `curl`? See the curl examples below.

**What needs your key:**

- **No key needed** — public market data: `GET /markets`, `GET /book`, `GET /midpoint`, etc.
- **Needs your key** — anything touching *your account*: orders, positions, balance (`401` otherwise).

### Quick curl examples (Polymarket style)

```bash
# 1. See markets (no key)
curl https://poly.teddytennant.com/markets

# 2. Place an order
curl -X POST https://poly.teddytennant.com/order \
  -H "POLY_API_KEY: <your_api_key>" -H "Content-Type: application/json" \
  -d '{"token_id":"<token>","side":"BUY","price":0.55,"size":10}'

# 3. Check positions
curl https://poly.teddytennant.com/positions -H "POLY_API_KEY: <your_api_key>"
```

(`Authorization: Bearer <key>` also works.)

Made a mess? Ask an admin to reset your balance to the starting $25,000.

---

<details>
<summary><strong>For club admins</strong> — running the stack, issuing keys, deploying</summary>

### Repository structure

```
predlab/
├── polymarket-sim/   # Polymarket-style Gamma + CLOB mock (Python / FastAPI)  :8001
├── leaderboard-rs/   # Public live leaderboard page (Rust / axum)             :8003
├── ratatui-admin/    # Admin TUI (Rust): issue keys + manage club roster
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
