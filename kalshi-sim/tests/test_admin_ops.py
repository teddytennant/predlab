"""Admin teaching ops: balance reset (one/all, clean slate) and member removal."""

from __future__ import annotations

from kalshi_sim.models.db import Order, PaperAccount, Position, User

RESET = "/trade-api/v2/admin/reset-user"
DELETE = "/trade-api/v2/admin/delete-user"


def test_reset_requires_admin(client):
    assert client.post(RESET, params={"username": "x"}).status_code == 403


def test_delete_requires_admin(client):
    assert client.post(DELETE, params={"username": "x"}).status_code == 403


def test_reset_member_clears_cash_orders_and_positions(
    client, session, make_user, make_market, admin_secret, starting_balance
):
    user = make_user(session, "alice")
    mkt = make_market(session, ticker="MKT")
    session.add(PaperAccount(user_id=user.id, balance_cents=1234))
    session.add(Position(user_id=user.id, ticker=mkt.ticker, yes_contracts=7))
    session.add(
        Order(
            user_id=user.id, ticker=mkt.ticker, side="bid", action="buy",
            price_dollars="0.50", count=3, status="open",
        )
    )
    session.commit()

    r = client.post(RESET, params={"username": "alice"}, headers={"X-Kalshi-Sim-Admin": admin_secret})
    assert r.status_code == 200
    assert r.json()["new_balance_cents"] == starting_balance

    session.expire_all()
    pa = session.query(PaperAccount).filter_by(user_id=user.id).one()
    assert pa.balance_cents == starting_balance
    pos = session.query(Position).filter_by(user_id=user.id).one()
    assert pos.yes_contracts == 0 and pos.no_contracts == 0
    assert not session.query(Order).filter_by(user_id=user.id, status="open").all()


def test_reset_all_wipes_everyone(client, session, make_user, admin_secret, starting_balance):
    for name in ("a", "b", "c"):
        u = make_user(session, name)
        session.add(PaperAccount(user_id=u.id, balance_cents=1))
    session.commit()

    r = client.post(RESET, headers={"X-Kalshi-Sim-Admin": admin_secret})  # no username -> all
    assert r.status_code == 200
    assert r.json()["count"] == 3

    session.expire_all()
    for pa in session.query(PaperAccount).all():
        assert pa.balance_cents == starting_balance


def test_delete_user_removes_member_and_data(
    client, session, make_user, make_market, admin_secret
):
    user = make_user(session, "alice")
    mkt = make_market(session, ticker="MKT")
    session.add(PaperAccount(user_id=user.id, balance_cents=500))
    session.add(Position(user_id=user.id, ticker=mkt.ticker, yes_contracts=2))
    session.add(
        Order(
            user_id=user.id, ticker=mkt.ticker, side="bid", action="buy",
            price_dollars="0.50", count=1, status="open",
        )
    )
    session.commit()
    uid = user.id

    r = client.post(DELETE, params={"username": "alice"}, headers={"X-Kalshi-Sim-Admin": admin_secret})
    assert r.status_code == 200
    assert r.json()["deleted"] == "alice"

    session.expire_all()
    assert session.query(User).filter_by(username="alice").one_or_none() is None
    assert not session.query(PaperAccount).filter_by(user_id=uid).all()
    assert not session.query(Position).filter_by(user_id=uid).all()
    assert not session.query(Order).filter_by(user_id=uid).all()


def test_delete_unknown_user_404(client, admin_secret):
    r = client.post(DELETE, params={"username": "ghost"}, headers={"X-Kalshi-Sim-Admin": admin_secret})
    assert r.status_code == 404
