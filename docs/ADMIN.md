# PredLab — running the stack

For issuing keys, resetting balances, and resolving markets week-to-week, see the [operations runbook](OPERATIONS.md). This page covers repo structure, self-hosting, roles, and deployment.

## Repository structure

```
predlab/
├── polymarket-sim/   # Polymarket-style Gamma + CLOB mock (Python / FastAPI)  :8001
├── leaderboard-rs/   # Public live leaderboard page (Rust / axum)             :8003
├── ratatui-admin/    # Admin TUI (Rust): issue keys + manage club roster
├── predlab-tui/      # Member TUI (Rust): vim leaderboard + markets + portfolio
├── predlab-gui/      # Desktop GUI (Rust / egui): trade, portfolio, admin panel
├── predlab-py/       # Member starter client (Python, uv-packaged)
├── docker-compose.yml
└── Makefile
```

The simulator syncs live prices from the real public Polymarket APIs and exposes drop-in–compatible endpoints.

## Run the simulator

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab
docker compose up --build      # Polymarket -> :8001, leaderboard -> :8003
```

Or run directly for development:

```bash
cd polymarket-sim && pip install -e ".[dev]" && uvicorn polymarket_sim.main:app --port 8001
```

## Admin TUI

```bash
make install-admin     # cargo install --path ratatui-admin  -> `predlab` on PATH
predlab                # or: make admin
```

Three tabs (`l`/`h` or Tab to switch):

- **Issue key** — type a username, pick a role with `←/→`, `Enter` mints a paper API key on the sim, saves the member to the local roster (`~/.predlab/students.db`), and copies the credentials block to your clipboard.
- **Roster** — browse members. `c` copies creds, `r` resets selected balance, `R` resets everyone, `x` removes member (destructive actions require `y` confirm).
- **Leaderboard** — live ranking by paper net worth (`r` to refresh).

Env vars: `POLY_URL` (default http://localhost:8001), `PREDLAB_ADMIN_SECRET` (for `X-Admin-Secret`).

## Desktop GUI admin panel

The desktop app ([`predlab-gui`](../predlab-gui/), `make gui` / `make install-gui`) has an Admin view too: issue keys with a role picker (member/admin/owner), revoke keys, reset one or all balances, browse the server-side roster with per-member profiles, and force-resolve markets. It unlocks with the master secret in its Settings — or automatically when your own API key has the admin role.

## Roles

| Role     | Can do                                                            |
|----------|-------------------------------------------------------------------|
| `member` | Trade & view **their own** account only (default).                |
| `admin`  | Issue/revoke keys, reset balances, remove members. (e.g. the VP.) |
| `owner`  | Everything, incl. resolving markets and granting roles.           |

The **master secret** (`ADMIN_SECRET`) authenticates as `owner` — your bootstrap key. Only owners can mint `admin`/`owner` keys.

## Issuing keys (curl)

```bash
# Returns the api_key the member uses as POLY_API_KEY
curl -X POST "https://poly.teddytennant.com/admin/create-paper-key?username=alice&role=member" \
  -H "X-Admin-Secret: $PREDLAB_ADMIN_SECRET"
```

Add `&role=admin` (owner only) for admin keys.

## Admin operations

`GET /admin/leaderboard` (admin-gated) returns members ranked by net worth.

Resets / deletes (admin-gated):

| Action            | Polymarket endpoint                          |
|-------------------|----------------------------------------------|
| Reset one member  | `POST /admin/reset-balance?username=alice`   |
| Reset everyone    | `POST /admin/reset-balance`                  |
| Remove a member   | `POST /admin/delete-user?username=alice`     |

A reset clears orders/positions and returns cash to the starting balance. Remove is permanent.

## Testing

```bash
make test          # all suites
make test-sims     # pytest for the simulator
make test-admin    # cargo test for the TUI + registry
make lint
```

Simulator tests are fully offline (temp SQLite, no network).

## Deploying

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
