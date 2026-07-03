# Installing PredLab

Two things you can install: the **desktop GUI** (`predlab-gui`) and the **API client** (just `predlab.py` + your key — no install really needed beyond `pip`).

## Desktop GUI — macOS / Linux

The GUI is a native Rust/egui app. It's built with `cargo install --git`, so you need Rust installed and it compiles from source on your machine (no prebuilt binaries yet).

**Windows is not supported.** eframe/egui builds fine on Windows in principle, but this app isn't tested or maintained there. If you're on Windows and want the GUI, run it inside a Linux VM (e.g. UTM, VirtualBox, VMware, or WSL2 with WSLg for GUI passthrough), or dual-boot Linux. If you just want the API, see below — WSL alone is enough for that.

### 1. Install Rust

macOS and Linux use the same installer:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Verify: `cargo --version` (need a recent stable toolchain; edition 2024 requires Rust 1.85+).

### 2. Platform dependencies

**macOS:** none — Xcode Command Line Tools (`xcode-select --install`) if you don't already have them.

**Linux:** egui needs X11/Wayland dev headers and a few system libs. On Debian/Ubuntu:

```bash
sudo apt install libgtk-3-dev libxkbcommon-dev libssl-dev pkg-config
```

On Fedora:

```bash
sudo dnf install gtk3-devel libxkbcommon-devel openssl-devel
```

On Arch:

```bash
sudo pacman -S gtk3 libxkbcommon openssl pkgconf
```

(NixOS: add these via a flake devShell/`nix-shell -p` rather than system-wide — ask if you need the derivation.)

### 3. Install the app

```bash
cargo install --git https://github.com/teddytennant/predlab predlab-gui --locked
```

This puts a `predlab-gui` binary on your `~/.cargo/bin` (make sure that's on your `PATH`).

### 4. Run it

```bash
predlab-gui
```

First run walks you through a setup wizard — paste in the API key your club admin gave you. It points at the club's hosted server (`https://poly.teddytennant.com`) by default, so there's nothing else to configure.

### Updating

Re-run the same `cargo install` command — it rebuilds against the latest commit.

---

## API client — macOS / Linux / Windows (incl. WSL)

If you just want to trade programmatically, you don't need Rust or the GUI at all — the client is a small `uv`-packaged Python library and works anywhere Python runs, **including native Windows**.

### 1. Get the client

```bash
git clone https://github.com/teddytennant/predlab.git && cd predlab/predlab-py && uv sync
```

That's it — `uv sync` creates a `.venv` with `predlab` installed. No separate `pip install` step needed. (Don't have `uv`? `curl -LsSf https://astral.sh/uv/install.sh | sh`, or on Windows see [astral.sh/uv](https://astral.sh/uv).)

**Windows users:** native Python + `uv` works fine directly (no VM needed) — the API client has no OS-specific code. WSL also works if you prefer a Linux-flavored shell.

### 2. Use it

```bash
export POLY_API_KEY="pm_paper_REPLACE_ME"   # key your admin gave you
uv run python -c "from predlab import PolymarketClient; print(PolymarketClient().markets(limit=5))"
```

```python
from predlab import PolymarketClient

poly = PolymarketClient()  # reads POLY_API_KEY from the environment
markets = poly.markets(limit=5)
```

Full walkthrough (placing orders, checking positions, curl equivalents) is in [`docs/API.md`](../docs/API.md).

---

## Summary

| Platform | GUI | API |
|---|---|---|
| macOS | ✅ native, build from source via `cargo install` | ✅ `pip install requests` |
| Linux | ✅ native, build from source via `cargo install` | ✅ `pip install requests` |
| Windows | ❌ unsupported — use a Linux VM or dual-boot if you need the GUI | ✅ native Python, or WSL if preferred |
