"""Admin teaching ops: balance reset (clean slate) and member removal."""

from __future__ import annotations

from sqlalchemy import select

from polymarket_sim.models.db import Order, Position, User
from polymarket_sim.services.auth import create_demo_user_with_key, ensure_paper_account
from polymarket_sim.services.paper_trading import get_or_create_position, place_paper_order

TOKEN_YES = "100"


def test_reset_balance_requires_admin(client):
    assert client.post("/admin/reset-balance", params={"username": "x"}).status_code == 403


def test_delete_user_requires_admin(client):
    assert client.post("/admin/delete-user", params={"username": "x"}).status_code == 403


def test_reset_member_clears_cash_orders_and_positions(
    client, session, market, admin_secret, starting_balance
):
    alice = create_demo_user_with_key(session, "alice", role="member")[0]
    # An open resting order (escrows cash) plus a held position.
    place_paper_order(session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5)
    pos = get_or_create_position(session, alice, market, TOKEN_YES)
    pos.size = 10
    ensure_paper_account(session, alice).balance_usd = 123.45
    session.commit()
    assert session.execute(select(Position).where(Position.user_id == alice.id)).scalars().all()

    r = client.post(
        "/admin/reset-balance",
        params={"username": "alice"},
        headers={"X-Admin-Secret": admin_secret},
    )
    assert r.status_code == 200
    assert r.json()["balance"] == starting_balance

    session.expire_all()
    assert float(ensure_paper_account(session, alice).balance_usd) == starting_balance
    assert not session.execute(
        select(Position).where(Position.user_id == alice.id)
    ).scalars().all(), "positions cleared"
    open_orders = session.execute(
        select(Order).where(Order.user_id == alice.id, Order.status.in_(["open", "partial"]))
    ).scalars().all()
    assert not open_orders, "open orders cancelled"


def test_reset_all_wipes_everyone(client, session, admin_secret, starting_balance):
    for name in ("a", "b", "c"):
        u = create_demo_user_with_key(session, name, role="member")[0]
        ensure_paper_account(session, u).balance_usd = 1.0
    session.commit()

    r = client.post("/admin/reset-balance", headers={"X-Admin-Secret": admin_secret})
    assert r.status_code == 200
    assert r.json()["reset"] == "all"

    session.expire_all()
    for u in session.execute(select(User)).scalars().all():
        assert float(ensure_paper_account(session, u).balance_usd) == starting_balance


def test_delete_user_removes_member_and_data(client, session, market, admin_secret):
    alice = create_demo_user_with_key(session, "alice", role="member")[0]
    place_paper_order(session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5)
    session.commit()

    r = client.post(
        "/admin/delete-user",
        params={"username": "alice"},
        headers={"X-Admin-Secret": admin_secret},
    )
    assert r.status_code == 200
    assert r.json()["deleted"] == "alice"

    session.expire_all()
    assert session.execute(select(User).where(User.username == "alice")).scalar_one_or_none() is None
    assert not session.execute(select(Order)).scalars().all()
    assert not session.execute(select(Position)).scalars().all()


def test_delete_unknown_user_404(client, admin_secret):
    r = client.post(
        "/admin/delete-user",
        params={"username": "ghost"},
        headers={"X-Admin-Secret": admin_secret},
    )
    assert r.status_code == 404
