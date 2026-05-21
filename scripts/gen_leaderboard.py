#!/usr/bin/env python3
"""Regenerate the leaderboard table in README.md from the live PredLab API.

Fetches the admin standings from both deployed sims, merges them by username
into a combined paper net-worth ranking, and replaces the block between the
``<!-- LEADERBOARD:START -->`` / ``<!-- LEADERBOARD:END -->`` markers.

This is CI glue (zero third-party deps so it runs instantly on a GitHub runner);
it is intentionally not part of the simulators. Driven by
``.github/workflows/leaderboard.yml`` on an hourly cron.

Env: POLY_URL, KALSHI_URL (defaults to the deployed hosts) and the admin
secrets PREDLAB_ADMIN_SECRET (Polymarket) + PREDLAB_KALSHI_SECRET (Kalshi).
"""

from __future__ import annotations

import json
import os
import re
import sys
import urllib.request
from datetime import datetime, timezone

POLY_URL = os.environ.get("POLY_URL", "https://poly.teddytennant.com").rstrip("/")
KALSHI_URL = os.environ.get("KALSHI_URL", "https://kalshi.teddytennant.com").rstrip("/")
POLY_SECRET = os.environ.get("PREDLAB_ADMIN_SECRET", "")
KALSHI_SECRET = os.environ.get("PREDLAB_KALSHI_SECRET", "")

START = "<!-- LEADERBOARD:START -->"
END = "<!-- LEADERBOARD:END -->"
README = os.path.join(os.path.dirname(__file__), "..", "README.md")


def _get(url: str, header: str, secret: str) -> list[dict]:
    # Cloudflare 403s the default "Python-urllib" User-Agent, so set a named one.
    req = urllib.request.Request(
        url, headers={header: secret, "User-Agent": "predlab-leaderboard-bot"}
    )
    with urllib.request.urlopen(req, timeout=15) as resp:  # noqa: S310 (trusted hosts)
        return json.load(resp)


def fetch_rows() -> list[dict]:
    """Combined ranking: [{username, poly, kalshi, total}], highest total first."""
    poly = _get(f"{POLY_URL}/admin/leaderboard", "X-Admin-Secret", POLY_SECRET)
    kalshi = _get(
        f"{KALSHI_URL}/trade-api/v2/admin/leaderboard", "X-Kalshi-Sim-Admin", KALSHI_SECRET
    )

    merged: dict[str, dict] = {}
    for entry in poly:
        merged.setdefault(entry["username"], {"poly": 0.0, "kalshi": 0.0})["poly"] = entry[
            "net_worth"
        ]
    for entry in kalshi:
        merged.setdefault(entry["username"], {"poly": 0.0, "kalshi": 0.0})["kalshi"] = entry[
            "net_worth"
        ]

    rows = [
        {"username": u, "poly": v["poly"], "kalshi": v["kalshi"], "total": v["poly"] + v["kalshi"]}
        for u, v in merged.items()
    ]
    rows.sort(key=lambda r: r["total"], reverse=True)
    return rows


def _money(value: float) -> str:
    return f"${value:,.2f}"


def render(rows: list[dict]) -> str:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    lines = [
        START,
        "",
        f"_Auto-updated {ts} — combined paper net worth across both simulators._",
        "",
        "| # | Member | Polymarket | Kalshi | Total |",
        "|--:|:-------|-----------:|-------:|------:|",
    ]
    medals = {1: "🥇", 2: "🥈", 3: "🥉"}
    for rank, r in enumerate(rows, start=1):
        badge = medals.get(rank, str(rank))
        lines.append(
            f"| {badge} | {r['username']} | {_money(r['poly'])} | "
            f"{_money(r['kalshi'])} | **{_money(r['total'])}** |"
        )
    if not rows:
        lines.append("| – | _no members yet_ | | | |")
    lines += ["", END]
    return "\n".join(lines)


def main() -> int:
    if not POLY_SECRET or not KALSHI_SECRET:
        print("error: set PREDLAB_ADMIN_SECRET and PREDLAB_KALSHI_SECRET", file=sys.stderr)
        return 1

    block = render(fetch_rows())
    path = os.path.abspath(README)
    with open(path, encoding="utf-8") as f:
        content = f.read()

    pattern = re.compile(re.escape(START) + r".*?" + re.escape(END), re.DOTALL)
    if not pattern.search(content):
        print(f"error: leaderboard markers not found in {path}", file=sys.stderr)
        return 1

    # lambda replacement avoids re interpreting backslashes/group refs in the block.
    updated = pattern.sub(lambda _m: block, content)
    if updated != content:
        with open(path, "w", encoding="utf-8") as f:
            f.write(updated)
        print("README leaderboard updated.")
    else:
        print("No change.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
