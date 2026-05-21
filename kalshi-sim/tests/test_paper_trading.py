"""Accounting tests for the cents-based Kalshi paper engine."""

from __future__ import annotations

import pytest

from kalshi_sim.models.api import CreateOrderRequestV2
from kalshi_sim.models.db import Position
from kalshi_sim.services.paper_trading import PaperTradingService


def _order(ticker, side, price, count, coid):
    return CreateOrderRequestV2(
        ticker=ticker, client_order_id=coid, side=side, count=count, price=price
    )


def _yes_contracts(session, user_id, ticker) -> int:
    pos = (
        session.query(Position)
        .filter(Position.user_id == user_id, Position.ticker == ticker)
        .first()
    )
    return pos.yes_contracts if pos else 0


def test_starting_balance(session, make_user, starting_balance):
    alice = make_user(session, "alice")
    svc = PaperTradingService(session)
    assert svc.get_balance(alice.id).balance == starting_balance


def test_buy_with_no_liquidity_rests_without_debit(session, make_user, make_market, starting_balance):
    alice = make_user(session, "alice")
    mkt = make_market(session)
    svc = PaperTradingService(session)
    resp = svc.place_order(alice.id, _order(mkt.ticker, "bid", "0.40", "5", "c1"))
    assert resp.fill_count == "0.00"
    assert resp.remaining_count == "5.00"
    # Kalshi engine only debits cash on a fill, so a resting bid leaves balance intact.
    assert svc.get_balance(alice.id).balance == starting_balance


def test_buy_beyond_balance_rejected(session, make_user, make_market):
    alice = make_user(session, "alice")
    mkt = make_market(session)
    svc = PaperTradingService(session)
    with pytest.raises(ValueError, match="insufficient paper balance"):
        svc.place_order(alice.id, _order(mkt.ticker, "bid", "0.99", "1000000", "c1"))


def test_order_on_untradable_market_rejected(session, make_user, make_market):
    alice = make_user(session, "alice")
    mkt = make_market(session, ticker="SETTLED-MKT", status="settled")
    svc = PaperTradingService(session)
    with pytest.raises(ValueError, match="not tradable"):
        svc.place_order(alice.id, _order(mkt.ticker, "bid", "0.50", "5", "c1"))


def test_buy_crosses_resting_ask_and_moves_cash_both_sides(
    session, make_user, make_market, starting_balance
):
    maker = make_user(session, "maker")
    taker = make_user(session, "taker")
    mkt = make_market(session)
    svc = PaperTradingService(session)

    # Maker rests an ask (sell yes) at 0.30; taker crosses with a 0.50 bid.
    svc.place_order(maker.id, _order(mkt.ticker, "ask", "0.30", "10", "m1"))
    resp = svc.place_order(taker.id, _order(mkt.ticker, "bid", "0.50", "10", "t1"))

    assert resp.fill_count == "10.00"
    # Fill happens at the maker's 0.30 = 30c each, 10 contracts = 300c.
    assert svc.get_balance(taker.id).balance == starting_balance - 300
    assert svc.get_balance(maker.id).balance == starting_balance + 300
    assert _yes_contracts(session, taker.id, mkt.ticker) == 10
    assert _yes_contracts(session, maker.id, mkt.ticker) == -10


def test_cancel_resting_order(session, make_user, make_market):
    alice = make_user(session, "alice")
    mkt = make_market(session)
    svc = PaperTradingService(session)
    resp = svc.place_order(alice.id, _order(mkt.ticker, "bid", "0.40", "5", "c1"))
    result = svc.cancel_order(alice.id, resp.order_id)
    assert result["cancelled"] is True
    assert result["removed_from_book"] is True


def test_admin_force_resolve_yes_pays_winners(
    session, make_user, make_market, starting_balance
):
    maker = make_user(session, "maker")
    taker = make_user(session, "taker")
    mkt = make_market(session)
    svc = PaperTradingService(session)
    svc.place_order(maker.id, _order(mkt.ticker, "ask", "0.30", "10", "m1"))
    svc.place_order(taker.id, _order(mkt.ticker, "bid", "0.50", "10", "t1"))

    out = svc.admin_force_resolve(mkt.ticker, "yes")
    assert out["result"] == "yes"
    # Taker held +10 yes -> +100c each on a YES resolution.
    assert svc.get_balance(taker.id).balance == (starting_balance - 300) + 1000
    # Settlement is zero-sum across the two paper accounts.
    assert (
        svc.get_balance(taker.id).balance + svc.get_balance(maker.id).balance
        == 2 * starting_balance
    )


def test_admin_force_resolve_validates_result(session, make_market):
    # Authorization now lives at the route layer; the service still validates input.
    mkt = make_market(session)
    svc = PaperTradingService(session)
    with pytest.raises(ValueError):
        svc.admin_force_resolve(mkt.ticker, "maybe")
