"""Accounting tests for the paper trading engine (balance, escrow, P&L, settlement)."""

from __future__ import annotations

import pytest

from polymarket_sim.services.auth import create_demo_user_with_key, ensure_paper_account
from polymarket_sim.services.paper_trading import (
    cancel_paper_order,
    force_resolve_market,
    get_or_create_position,
    place_paper_order,
    update_position_on_fill,
)

TOKEN_YES = "100"


def _user(session, name):
    user, _key, _secret = create_demo_user_with_key(session, name, name.title())
    return user


def _balance(session, user) -> float:
    return float(ensure_paper_account(session, user).balance_usd)


def test_new_account_starts_with_configured_balance(session, starting_balance):
    alice = _user(session, "alice")
    assert _balance(session, alice) == starting_balance


def test_buy_escrows_notional_and_rests_when_no_liquidity(session, market, starting_balance):
    alice = _user(session, "alice")
    order = place_paper_order(
        session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5
    )
    # 5 shares @ 0.40 = 2.00 reserved up front; nothing to match -> open.
    assert order.status == "open"
    assert float(order.filled_size) == 0.0
    assert _balance(session, alice) == pytest.approx(starting_balance - 2.0)


def test_cancel_buy_refunds_escrow(session, market, starting_balance):
    alice = _user(session, "alice")
    order = place_paper_order(
        session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5
    )
    assert _balance(session, alice) == pytest.approx(starting_balance - 2.0)
    cancelled = cancel_paper_order(session, alice, order.id)
    assert cancelled.status == "cancelled"
    assert _balance(session, alice) == pytest.approx(starting_balance)


def test_buy_beyond_balance_is_rejected(session, market, starting_balance):
    alice = _user(session, "alice")
    with pytest.raises(ValueError, match="Insufficient paper balance"):
        place_paper_order(
            session, alice, market.id, TOKEN_YES, side="buy", price=1.0, size=1_000_000
        )
    # Balance untouched after a rejected order.
    assert _balance(session, alice) == starting_balance


def test_buy_fills_against_resting_ask_and_builds_position(session, market):
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    # Maker rests an ask; taker crosses it.
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    order = place_paper_order(
        session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10
    )
    assert order.status == "filled"
    assert float(order.filled_size) == 10
    # Taker now holds 10 shares at the maker's fill price.
    pos = get_or_create_position(session, taker, market, TOKEN_YES)
    assert float(pos.size) == 10
    assert float(pos.avg_entry_price) == pytest.approx(0.30)


def test_sell_fill_credits_cash(session, market):
    maker = _user(session, "maker")
    seller = _user(session, "seller")
    # Maker rests a bid; seller crosses it and gets paid the bid price.
    place_paper_order(session, maker, market.id, TOKEN_YES, side="buy", price=0.60, size=10)
    before = _balance(session, seller)
    order = place_paper_order(
        session, seller, market.id, TOKEN_YES, side="sell", price=0.50, size=10
    )
    assert order.status == "filled"
    assert _balance(session, seller) == pytest.approx(before + 0.60 * 10)


def test_resting_maker_is_settled_on_fill(session, market, starting_balance):
    """Both sides of a cross must settle — not just the incoming taker.

    Before the fix the resting maker was credited nothing and its order stayed
    'open' forever. Now it is paid the fill proceeds and marked filled. (Position
    still clamps at 0 under the current no-shorts MVP.)
    """
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    maker_order = place_paper_order(
        session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10
    )
    assert _balance(session, maker) == pytest.approx(starting_balance)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)

    session.refresh(maker_order)
    assert maker_order.status == "filled"
    assert float(maker_order.filled_size) == pytest.approx(10)
    assert _balance(session, maker) == pytest.approx(starting_balance + 0.30 * 10)


def test_vwap_average_entry_updates_across_fills(session, market):
    alice = _user(session, "alice")
    update_position_on_fill(session, alice, market, TOKEN_YES, 10, 0.40, "buy")
    update_position_on_fill(session, alice, market, TOKEN_YES, 10, 0.60, "buy")
    pos = get_or_create_position(session, alice, market, TOKEN_YES)
    assert float(pos.size) == 20
    assert float(pos.avg_entry_price) == pytest.approx(0.50)  # VWAP of 0.40 and 0.60
    # Selling part of the position keeps the average entry.
    update_position_on_fill(session, alice, market, TOKEN_YES, 5, 0.55, "sell")
    assert float(pos.size) == 15
    assert float(pos.avg_entry_price) == pytest.approx(0.50)


def test_force_resolve_yes_pays_winning_holders_one_dollar_per_share(session, market):
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)
    bal_before = _balance(session, taker)

    result = force_resolve_market(session, market.id, resolution="yes")
    assert result["positions_settled"] >= 1
    # 10 winning shares pay out $1 each.
    assert _balance(session, taker) == pytest.approx(bal_before + 10.0)
    # Market is closed afterwards.
    session.refresh(market)
    assert market.closed is True and market.active is False
