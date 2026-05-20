# PredLab

**PredLab** is the unified tooling and paper trading environment for the school Prediction Markets Club.

Students can practice real trading strategies on **both** Polymarket-style and Kalshi-style markets using paper money, while the club admins have powerful tools for onboarding, oversight, and teaching.

## Repository Structure

```
predlab/
├── polymarket-sim/       # Full mock of Polymarket's Gamma + CLOB APIs
├── kalshi-sim/           # Full mock of Kalshi's Trade API v2
├── src/
│   └── predlab/          # The PredLab TUI (admin + student overview)
│       ├── cli.py
│       └── tui.py
├── docker-compose.yml    # One-command startup for both simulators
├── pyproject.toml        # Makes `predlab` an installable command
└── README.md
```

## Quick Start

### 1. Start both simulators

```bash
docker compose up --build
```

- Polymarket simulator → http://localhost:8001
- Kalshi simulator → http://localhost:8002

### 2. Install the PredLab TUI (recommended)

From the repo root:

```bash
pip install -e .
```

Then just run:

```bash
predlab
```

The TUI supports two modes:
- **Admin mode** (press Enter at the key prompt) — create students, issue dual keys, resets, force resolves, etc.
- **Student / Read-only mode** (paste a paper key) — Club Overview + Leaderboard only.

## Student Experience

When an admin creates a student in `predlab`, the student receives:
- One username
- A key for the Polymarket simulator
- A key for the Kalshi simulator

They can then:
- Point the official SDKs at either simulator
- Trade paper money on either (or both) platforms
- Use the `predlab` TUI in read-only mode to see the club leaderboard and overview

## Philosophy

- Real APIs → Students build against the actual SDKs with only the base URL changed.
- Paper money only → Zero financial risk.
- One identity across platforms → Students can experiment with strategies on both market designs.
- Admin tooling that doesn't get in the way of students.

---

For club admins: run `predlab` and choose the admin path. All student management and oversight lives there.