# PredLab

Unified tooling for the school Prediction Markets Club.

## Structure

- `polymarket-sim/` — Full mock of the Polymarket API (paper trading)
- `kalshi-sim/` — Full mock of the Kalshi Trade API v2 (paper trading)
- `admin/tui/` — PredLab TUI (the club admin + student overview tool)

## Quick Start

```bash
# Run both simulators
docker compose up

# Run the admin TUI
./admin/tui/predlab.py
# or after installing: predlab
```

Students get keys for **both** simulators from the TUI and can trade on either (or both) using real SDKs pointed at the local instances.
