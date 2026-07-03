# predlab (Python client)

Trade the PredLab paper simulator from Python. One class, three dependencies deep (just `requests`).

## Install

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab/predlab-py && uv sync
```

That clones the repo and gets a ready-to-use virtualenv in `.venv` with `predlab` installed.

Prefer to add it into your own uv project instead of cloning standalone?

```bash
uv add "predlab @ git+https://github.com/teddytennant/predlab.git#subdirectory=predlab-py"
```

## Use it

```bash
export POLY_API_KEY="pm_paper_..."   # key your admin gave you
uv run python -c "from predlab import PolymarketClient; print(PolymarketClient().markets(limit=1))"
```

```python
from predlab import PolymarketClient

poly = PolymarketClient()  # reads POLY_API_KEY from the environment automatically
markets = poly.markets(limit=5)
m = markets[0]
yes_token = m["clobTokenIds"][0]

poly.place_order(token_id=yes_token, side="BUY", price=0.55, size=10)
print(poly.portfolio())
```

Full walkthrough (order status, troubleshooting, curl equivalents): [`../docs/API.md`](../docs/API.md)
