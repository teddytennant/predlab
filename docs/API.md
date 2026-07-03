# PredLab API — member walkthrough

Trade programmatically against the club's hosted server at `https://poly.teddytennant.com`.

You need an **API key** from your admin — a string like `pm_paper_xxxxxxxx`, sent as the `POLY_API_KEY` header on any request that touches your account.

> **New to prediction markets?** A market asks a yes/no question ("Will X happen?"). You buy **YES** or **NO** shares priced between **$0.01 and $0.99** — the price *is* the market's estimated probability. Each winning share pays out **$1.00** when the market resolves; losers pay $0. Buy low on an outcome you believe in, and you profit if you're right.

## Step 0 — Get the client

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab/predlab-py && uv sync
export POLY_API_KEY="pm_paper_REPLACE_ME"   # key your admin gave you
```

`uv sync` builds a `.venv` with `predlab` installed — no separate `pip install` step. Don't have `uv`? `curl -LsSf https://astral.sh/uv/install.sh | sh`. Prefix commands below with `uv run` (e.g. `uv run python`), or `source .venv/bin/activate` first.

## Step 1 — Plug in your key

```python
from predlab import PolymarketClient

poly = PolymarketClient()   # reads POLY_API_KEY from the environment automatically
```

`poly` now talks to the club server. (To point at a local sim instead, set the `POLY_BASE` environment variable.)

## Step 2 — Browse markets and grab a token

You trade **outcome tokens**, not markets. Every market has a `clobTokenIds` list: index **`0` = YES**, index **`1` = NO**. You need one of those token ids to place an order.

```python
markets = poly.markets(limit=5)        # browsing needs no key
m = markets[0]
print(m["question"])                   # "Will ... ?"
print(m["bestBid"], m["bestAsk"])      # current market price (0–1)

yes_token = m["clobTokenIds"][0]       # the YES outcome token
no_token  = m["clobTokenIds"][1]       # the NO outcome token
```

## Step 3 — Place an order

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

## Step 4 — Check your account

```python
print(poly.positions())   # the shares you hold + unrealized profit/loss
```

For your cash and total net worth (the number that ranks you):

```bash
curl https://poly.teddytennant.com/portfolio -H "POLY_API_KEY: pm_paper_REPLACE_ME"
# -> {"cash": 24994.5, "positions_value": 5.5, "open_orders_value": 0.0, "net_worth": 25000.0}
```

**Net worth = free cash + your positions marked at the current market price + cash escrowed in your resting buy orders.** That's your leaderboard score.

## Step 5 — Climb the leaderboard

Watch your standing at **[predlab.teddytennant.com](https://predlab.teddytennant.com)** — it refreshes automatically.

## Prefer curl or another language?

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

## Troubleshooting

| Symptom | What it means / fix |
|---|---|
| `401 Unauthorized` | Key missing, mistyped, or revoked. Re-check the `POLY_API_KEY` header or ask your admin. |
| Order stays `open`, never fills | No one has taken the other side yet. Move your price toward `bestBid`/`bestAsk`, or wait for club activity. |
| `unknown token` error | You passed a market `id` — use a value from that market's `clobTokenIds` instead. |
| Wrecked your balance | Ask an admin to reset you to the starting **$25,000**. |
