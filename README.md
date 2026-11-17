# PredLab

Paper trading for our school's prediction markets club. Everyone gets $25,000
of fake money to trade Polymarket-style yes/no markets, and the leaderboard is
at [predlab.teddytennant.com](https://predlab.teddytennant.com). It's fake
money and has nothing to do with Polymarket.

To trade you need an API key from a club admin (`pm_paper_...`).

## Python

```bash
git clone https://github.com/teddytennant/predlab.git
cd predlab/predlab-py && uv sync
export POLY_API_KEY=pm_paper_xxxxxxxx
uv run python -c "from predlab import PolymarketClient; print(PolymarketClient().markets(limit=1))"
```

Works on Windows too. [docs/API.md](docs/API.md) covers orders, your
portfolio and curl.

## Desktop app

macOS and Linux only. On Windows use a VM or stick to the Python client.

```bash
cargo install --git https://github.com/teddytennant/predlab predlab-gui --locked
predlab-gui
```

It asks for your key the first time. [predlab-gui/INSTALL.md](predlab-gui/INSTALL.md)
has the Rust and system dependencies.

## Admins

[docs/ADMIN.md](docs/ADMIN.md) is self-hosting, keys and roles, and
[docs/OPERATIONS.md](docs/OPERATIONS.md) is running the weekly competitions.
[docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md) explains how the
simulator works.
