# PredLab starter kit (for members)

You got a Polymarket paper-trading API key from a club admin. No big SDK needed —
[`predlab.py`](predlab.py) is the entire client (just `requests`).

## 1. Install the dependency

```bash
pip install -r requirements.txt        # requests
```

## 2. Set your key

```bash
export POLY_KEY="pm_paper_..."          # the key your admin gave you
```

(Override `POLY_BASE` if hitting a local simulator instead of the club host.)

## 3. Verify it works

```bash
python predlab.py
```

You should see a live market and (if your key is set) your positions / balance.

## 4. Trade from Python

```python
from predlab import PolymarketClient

poly = PolymarketClient(api_key="pm_paper_yourkey")
print(poly.markets(limit=3))
poly.place_order(token_id="...", side="BUY", price=0.62, size=10)
```

See the class methods in `predlab.py` for `book`, `positions`, `place_order`, etc.
Start with $25,000 paper; climb the [leaderboard](https://predlab.teddytennant.com).

## 5. Notes

- Prices are 0–1 (a YES share pays $1 if it resolves your way).
- You start with **$25,000** paper. Your net worth appears on the club leaderboard.
- The simulator is a faithful mock of the real Polymarket APIs — your code will work against the live exchange with only a host + key change.
