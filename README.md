# PredLab

**PredLab** is the paper-trading playground for the school Prediction Markets Club. Trade Polymarket-style yes/no markets with $25,000 of fake money, practice real strategies, and climb the club leaderboard against everyone else in the club.

> **PAPER TRADING ONLY — NOT AFFILIATED WITH POLYMARKET.** Educational use, fake money.

🏆 **[predlab.teddytennant.com](https://predlab.teddytennant.com)** — live standings, updated automatically.

Nothing to install to just watch. To trade, your admin gives you an API key and you pick one of the clients below.

## Install

You'll need an API key from a club admin first (`pm_paper_xxxxxxxx`).

### API (any OS, incl. Windows) — one liner

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab/predlab-py && uv sync
```

That clones the repo and installs the `predlab` Python package into a ready-to-use `.venv` (packaged with [`uv`](https://astral.sh/uv) — no `pip install` step). Then:

```bash
export POLY_API_KEY="pm_paper_xxxxxxxx"     # key your admin gave you
uv run python -c "from predlab import PolymarketClient; print(PolymarketClient().markets(limit=1))"
```

Full walkthrough — placing orders, checking your portfolio, curl examples: **[docs/API.md](docs/API.md)**

### Desktop GUI (macOS / Linux)

```bash
cargo install --git https://github.com/teddytennant/predlab predlab-gui --locked
predlab-gui
```

A first-run wizard walks you through pasting your key. Full setup steps (Rust toolchain, platform deps): **[predlab-gui/INSTALL.md](predlab-gui/INSTALL.md)**

**Windows isn't supported for the GUI.** If you want it, run Linux in a VM (UTM, VirtualBox, VMware) or dual-boot. If you just want the API, native Windows or WSL both work fine.

## More docs

- **[docs/API.md](docs/API.md)** — full member API walkthrough, troubleshooting
- **[predlab-gui/INSTALL.md](predlab-gui/INSTALL.md)** — desktop GUI install (macOS/Linux/Windows notes)
- **[docs/PROJECT_OVERVIEW.md](docs/PROJECT_OVERVIEW.md)** — architecture, how the simulator works
- **[docs/OPERATIONS.md](docs/OPERATIONS.md)** — running weekly competitions (for admins)
- **[docs/ADMIN.md](docs/ADMIN.md)** — self-hosting, issuing keys, roles, deploying
