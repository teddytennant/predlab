# PredLab starter kit (for members)

You got a key from a club admin — here's how to actually trade with it. There's no big
SDK to install: [`predlab.py`](predlab.py) is the whole client.

## 1. Install the two dependencies

```bash
pip install -r requirements.txt        # requests + cryptography
```

## 2. Set your credentials

Your admin gave you a **Polymarket API key** and, for Kalshi, a **key id** plus a
**private key file** (`you.pem` — keep it private, it's only issued once).

```bash
export POLY_KEY="pm_paper_..."          # your Polymarket API key
export KALSHI_KEY_ID="ks_live_..."      # your Kalshi key id
export KALSHI_PEM="./you.pem"           # path to your Kalshi private key file
```

## 3. Check it works

```bash
python predlab.py
```

You should see your Polymarket positions and your Kalshi balance ($25,000 to start).

## 4. Trade

```python
from predlab import PolymarketClient, KalshiClient

# --- Polymarket: auth is just your key in a header ---
poly = PolymarketClient(api_key="pm_paper_...")
print(poly.markets(limit=5))                       # browse (public)
poly.place_order(token_id="<token>", side="BUY", price=0.55, size=10)
print(poly.positions())

# --- Kalshi: the client signs every request with your .pem for you ---
kal = KalshiClient(key_id="ks_live_...", private_key_pem_path="you.pem")
print(kal.balance())
print(kal.markets(limit=5))
kal.create_order(ticker="<TICKER>", side="bid", count=10, price=0.65)   # bid = buy YES
print(kal.positions())
```

Prices are in dollars from 0 to 1 (a contract pays out $1 if it resolves your way). Both
platforms start you at **$25,000 of paper money**. Your combined net worth shows up on the
[club leaderboard](../README.md#-leaderboard).

> Want the real thing instead? The official **Kalshi Python SDK** also works — point it at
> `https://kalshi.teddytennant.com/trade-api/v2` with your key id and `.pem`. Polymarket is
> simple enough that plain `requests`/`curl` with the `POLY_API_KEY` header is easiest.
