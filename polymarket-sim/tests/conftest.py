"""
Shared pytest fixtures for polymarket-sim.

The app builds its SQLAlchemy engine at import time from ``DATABASE_URL`` and the
FastAPI lifespan performs a live network sync on startup. To keep tests fast,
deterministic, and offline we:

* point ``DATABASE_URL`` at an isolated temp SQLite file *before* importing the app, and
* drive the app with ``TestClient(app)`` *without* the ``with`` block, so the
  lifespan (and its network sync) never runs. Tables are created directly here.
"""

from __future__ import annotations

import os
import tempfile

# Must be set before importing any polymarket_sim module (config/db read env at import).
_TMPDIR = tempfile.mkdtemp(prefix="polysim-test-")
os.environ["DATABASE_URL"] = f"sqlite:///{_TMPDIR}/test.db"
os.environ["ENVIRONMENT"] = "test"
os.environ["ADMIN_SECRET"] = "test-admin-secret"
os.environ["STARTING_BALANCE_USD"] = "25000.0"

import pytest  # noqa: E402

import polymarket_sim.models.db  # noqa: E402,F401  (registers all ORM models on Base)
from polymarket_sim.db import Base, SessionLocal, engine  # noqa: E402
from polymarket_sim.models.db import Market  # noqa: E402
from polymarket_sim.services import orderbook as ob  # noqa: E402

ADMIN_SECRET = "test-admin-secret"
STARTING_BALANCE = 25000.0


@pytest.fixture(autouse=True)
def _clean_state():
    """Fresh schema + cleared in-memory order books around every test."""
    Base.metadata.drop_all(bind=engine)
    Base.metadata.create_all(bind=engine)
    ob.reset_orderbook()
    yield
    ob.reset_orderbook()


@pytest.fixture
def session():
    s = SessionLocal()
    try:
        yield s
    finally:
        s.close()


@pytest.fixture
def starting_balance() -> float:
    return STARTING_BALANCE


@pytest.fixture
def admin_secret() -> str:
    return ADMIN_SECRET


@pytest.fixture
def make_market():
    """Factory: insert a deterministic market (no Gamma sync needed)."""

    def _make(
        session,
        market_id: str = "1",
        token_yes: str = "100",
        token_no: str = "101",
        best_bid: float = 0.5,
        best_ask: float = 0.5,
        last_trade_price: float = 0.5,
    ) -> Market:
        m = Market(
            id=market_id,
            condition_id=f"0xcond{market_id}",
            question=f"Will test {market_id} happen?",
            slug=f"will-test-{market_id}",
            outcomes=["Yes", "No"],
            outcome_prices=["0.5", "0.5"],
            clob_token_ids=[token_yes, token_no],
            best_bid=best_bid,
            best_ask=best_ask,
            last_trade_price=last_trade_price,
            volume=1000.0,
            liquidity=500.0,
            active=True,
            closed=False,
        )
        session.add(m)
        session.commit()
        return m

    return _make


@pytest.fixture
def market(session, make_market) -> Market:
    return make_market(session)


@pytest.fixture
def client():
    """TestClient that does NOT trigger the lifespan (no network sync)."""
    from fastapi.testclient import TestClient

    from polymarket_sim.main import app

    return TestClient(app)
