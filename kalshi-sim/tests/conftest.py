"""
Shared pytest fixtures for kalshi-sim.

Like the live app, the engine reads ``DATABASE_URL`` at import and the lifespan
performs a network sync on startup. Tests stay offline by:

* pointing ``DATABASE_URL`` at an isolated temp SQLite file before import, and
* driving the app with ``TestClient(app)`` (no ``with`` block), so the lifespan /
  network sync never runs.

``init_db()`` deletes and recreates the SQLite file on each call, giving every
test a clean schema. The in-memory order books are cleared too.
"""

from __future__ import annotations

import os
import tempfile

_TMPDIR = tempfile.mkdtemp(prefix="kalshisim-test-")
os.environ["DATABASE_URL"] = f"sqlite:///{_TMPDIR}/test.db"
os.environ["ENVIRONMENT"] = "test"
os.environ["CLUB_ADMIN_SECRET"] = "test-admin-secret"
os.environ["STARTING_BALANCE_CENTS"] = "2500000"
os.environ["DEV_BYPASS_AUTH"] = "true"

import pytest  # noqa: E402

import kalshi_sim.db as kdb  # noqa: E402
import kalshi_sim.services.orderbook as ob  # noqa: E402
from kalshi_sim.config import get_settings  # noqa: E402
from kalshi_sim.models.db import Market, User  # noqa: E402

ADMIN_SECRET = "test-admin-secret"
STARTING_BALANCE_CENTS = 2_500_000


@pytest.fixture(autouse=True)
def _clean_state():
    kdb.init_db()  # deletes + recreates the temp SQLite file -> clean schema
    ob._orderbooks.clear()
    yield
    ob._orderbooks.clear()


@pytest.fixture
def session():
    s = kdb._SessionLocal()
    try:
        yield s
    finally:
        s.close()


@pytest.fixture
def settings():
    return get_settings()


@pytest.fixture
def starting_balance() -> int:
    return STARTING_BALANCE_CENTS


@pytest.fixture
def admin_secret() -> str:
    return ADMIN_SECRET


@pytest.fixture
def make_user():
    def _make(session, username: str) -> User:
        u = User(username=username, display_name=username.title())
        session.add(u)
        session.commit()
        session.refresh(u)
        return u

    return _make


@pytest.fixture
def make_market():
    def _make(
        session,
        ticker: str = "TEST-MKT",
        status: str = "active",
        last_price_dollars: str = "0.5000",
    ) -> Market:
        m = Market(
            ticker=ticker,
            event_ticker="TEST-EVENT",
            title="Test market",
            yes_sub_title="Yes",
            no_sub_title="No",
            status=status,
            last_price_dollars=last_price_dollars,
        )
        session.add(m)
        session.commit()
        return m

    return _make


@pytest.fixture
def client():
    """TestClient that does NOT trigger the lifespan (no network sync)."""
    from fastapi.testclient import TestClient

    from kalshi_sim.main import app

    return TestClient(app)
