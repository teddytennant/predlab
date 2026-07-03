#!/usr/bin/env python
"""
Seed script for demo paper users + API keys.

Usage (from repo root, after pip install -e .[dev]):
    python scripts/seed_demo_users.py --username alice_quant --display "Alice Quant"

Prints the paper API key (use as POLY_API_KEY) and the one-time secret.
"""

from __future__ import annotations

import argparse
import os
import sys

# Ensure src on path when run directly
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "src")))

from sqlalchemy.orm import Session

from polymarket_sim.db import SessionLocal, init_db
from polymarket_sim.services.auth import create_demo_user_with_key


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--username", default="club_trader_1", help="Human-readable username for leaderboard")
    parser.add_argument("--display", default=None, help="Display name")
    args = parser.parse_args()

    init_db()
    db: Session = SessionLocal()
    try:
        user, key, secret = create_demo_user_with_key(db, args.username, args.display)
        print("\n=== PAPER TRADING ACCOUNT CREATED ===")
        print(f"Username:   {user.username}")
        print(f"Display:    {user.display_name}")
        print(f"API Key:    {key}     <--- put this in POLY_API_KEY header")
        print(f"Secret:     {secret}  <--- shown ONCE; for client creds (ignored by sim for paper keys)")
        print("\nExample curl:")
        print(f'  curl -H "POLY_API_KEY: {key}" http://localhost:8001/user/orders')
        print("\nPoint py-clob-client at http://localhost:8001 with this key (dummy secret ok).")
    except ValueError as ve:
        print("Error:", ve)
        sys.exit(1)
    finally:
        db.close()


if __name__ == "__main__":
    main()
