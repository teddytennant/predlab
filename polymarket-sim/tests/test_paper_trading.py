"""Accounting tests for the paper trading engine (balance, escrow, P&L, settlement)."""

from __future__ import annotations

import pytest

from polymarket_sim.services.auth import create_demo_user_with_key, ensure_paper_account
from polymarket_sim.services.paper_trading import (
    _current_price_for_token,
    cancel_paper_order,
    compute_net_worth,
    force_resolve_market,
    get_or_create_position,
    get_user_detail,
    place_paper_order,
    record_all_snapshots,
    record_snapshot,
    reset_user_to_starting,
    update_position_on_fill,
)

TOKEN_YES = "100"
TOKEN_NO = "101"


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
    'open' forever. Now it is paid the fill proceeds and marked filled.
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


def test_naked_sell_opens_short_and_does_not_mint_money(session, market, starting_balance):
    """A maker selling shares it doesn't hold opens a short, not free money.

    Regression: the sell fill used to clamp the position at 0 while crediting the
    proceeds — minting paper money and inflating net worth (old result: +$3). Now
    the position goes to -10 and is marked as a liability, so net worth reflects a
    real mark-to-market loss instead.
    """
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    # Maker rests an ask with no position; taker crosses -> maker is short 10.
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)

    pos = get_or_create_position(session, maker, market, TOKEN_YES)
    assert float(pos.size) == pytest.approx(-10)
    nw = compute_net_worth(session, maker)
    # Cash up by the 0.30*10 sale, but the -10 short marks at the 0.50 mid (-$5).
    assert nw["positions_value"] == pytest.approx(-5.0)
    # Net worth is starting + 3 (cash) - 5 (short) = starting - 2 — NOT starting + 3.
    assert nw["net_worth"] == pytest.approx(starting_balance - 2.0)


def test_cancelled_order_is_purged_from_book_and_cannot_refill(session, market, starting_balance):
    """Cancelling must remove the resting entry so a later cross can't fill it.

    Regression: the cancelled maker used to stay in the in-memory book, so an
    incoming order still matched it — double-crediting the canceller whose escrow
    had already been refunded.
    """
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    bid = place_paper_order(session, maker, market.id, TOKEN_YES, side="buy", price=0.60, size=10)
    cancel_paper_order(session, maker, bid.id)
    assert _balance(session, maker) == pytest.approx(starting_balance)  # escrow refunded

    # Taker sells into where that 0.60 bid used to be; it must not match the cancelled order.
    sell = place_paper_order(session, taker, market.id, TOKEN_YES, side="sell", price=0.50, size=10)
    assert sell.status == "open"
    assert float(sell.filled_size) == 0.0
    # Maker untouched — no phantom second fill / double credit.
    assert _balance(session, maker) == pytest.approx(starting_balance)


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


def test_net_worth_counts_cash_escrowed_in_open_buys(session, market, starting_balance):
    """A resting buy must not shrink net worth: escrowed cash is still the user's money."""
    alice = _user(session, "alice")
    place_paper_order(session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5)

    nw = compute_net_worth(session, alice)
    # Free cash dropped by the $2 escrow, but it reappears as open_orders_value.
    assert nw["cash"] == pytest.approx(starting_balance - 2.0)
    assert nw["positions_value"] == pytest.approx(0.0)
    assert nw["open_orders_value"] == pytest.approx(2.0)
    assert nw["net_worth"] == pytest.approx(starting_balance)


def test_limit_buy_filling_below_limit_refunds_over_escrow(session, market, starting_balance):
    """Buying through a cheaper ask must refund the escrow slack (limit - fill)."""
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    # Escrows 0.50*10 = 5.0 but fills at the maker's 0.30 -> 2.0 must come back.
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)

    # Cash debited is the actual fill cost (3.0), not the 5.0 escrow (the bug).
    assert _balance(session, taker) == pytest.approx(starting_balance - 0.30 * 10)
    # Net worth = remaining cash + position marked at the 0.50 mid (a real +2 paper gain).
    expected_nw = (starting_balance - 0.30 * 10) + 0.50 * 10
    assert compute_net_worth(session, taker)["net_worth"] == pytest.approx(expected_nw)


def test_market_buy_is_not_double_charged(session, make_market, starting_balance):
    """Market buys escrow at the mark then reconcile to the fill — never charged twice."""
    # Mark = mid = 0.50; a maker rests an ask well below it.
    mkt = make_market(session, market_id="2", best_bid=0.5, best_ask=0.5)
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    place_paper_order(session, maker, mkt.id, TOKEN_YES, side="sell", price=0.30, size=10)
    # price=None => market order. Old bug: escrow(5.0) + fill debit(3.0) = lost $5.
    order = place_paper_order(session, taker, mkt.id, TOKEN_YES, side="buy", price=None, size=10)

    assert order.status == "filled"
    # Only the actual fill cost (3.0) leaves cash — not escrow(5) + a second fill debit(3).
    assert _balance(session, taker) == pytest.approx(starting_balance - 0.30 * 10)
    expected_nw = (starting_balance - 0.30 * 10) + 0.50 * 10
    assert compute_net_worth(session, taker)["net_worth"] == pytest.approx(expected_nw)


def test_record_snapshot_and_user_detail_for_profile_page(session, market, starting_balance):
    """The /admin/user/{name} pipeline: snapshots, positions, trades all wired."""
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    # An initial snapshot per user (what startup seeds).
    record_all_snapshots(session)

    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)
    # The taker-fill path snapshots automatically; this confirms the maker hook too.
    detail = get_user_detail(session, "taker")
    assert detail is not None
    assert detail["username"] == "taker"
    assert detail["net_worth"] == pytest.approx(
        (starting_balance - 0.30 * 10) + 0.50 * 10
    )
    # Trades reflect the fill (one buy on the taker's side).
    assert len(detail["trades"]) == 1 and detail["trades"][0]["side"] == "buy"
    # Position is exposed with P&L.
    assert len(detail["positions"]) == 1
    assert detail["positions"][0]["size"] == pytest.approx(10)
    # The numeric fields must be JSON numbers, not Decimal/str — the Rust
    # leaderboard deserialises into typed f64 and would 500 on a string.
    assert isinstance(detail["positions"][0]["size"], float)
    # History has at least the seed point + the fill snapshot, and the latest
    # point matches current net worth.
    assert len(detail["history"]) >= 2
    assert detail["history"][-1]["net_worth"] == pytest.approx(detail["net_worth"])


def test_reset_records_a_snapshot_so_graph_drops_to_starting(session, market, starting_balance):
    """A reset is a real net-worth event — the graph must show the cliff."""
    alice = _user(session, "alice")
    place_paper_order(session, alice, market.id, TOKEN_YES, side="buy", price=0.40, size=5)
    record_snapshot(session, alice)  # explicit pre-reset point
    reset_user_to_starting(session, alice, starting_balance)

    detail = get_user_detail(session, "alice")
    assert detail is not None
    assert detail["net_worth"] == pytest.approx(starting_balance)
    # Last snapshot reflects the post-reset cliff back to the starting balance.
    assert detail["history"][-1]["net_worth"] == pytest.approx(starting_balance)


def test_get_user_detail_returns_none_for_unknown_user(session):
    assert get_user_detail(session, "no-such-member") is None


def test_no_leg_token_is_valued_at_one_minus_yes_mid(session, make_market):
    """Holding the second (No) outcome token must mark at 1 - the Yes-leg mid.

    Gamma's bestBid/bestAsk describe the Yes leg, so a No-leg holding was
    previously silently valued at the dominant Yes-leg price — making lossy
    No-leg positions read as huge winners.
    """
    # Yes mid = 0.81 means No should mark at 0.19.
    mkt = make_market(
        session, market_id="legs", token_yes="YES_TOK", token_no="NO_TOK",
        best_bid=0.80, best_ask=0.82,
    )
    assert _current_price_for_token(session, mkt.clob_token_ids[0]) == pytest.approx(0.81)
    assert _current_price_for_token(session, mkt.clob_token_ids[1]) == pytest.approx(0.19)


def test_net_worth_uses_correct_leg_for_no_position(session, make_market, starting_balance):
    """End-to-end: a position on the No leg must price its leg, not the Yes leg."""
    mkt = make_market(
        session, market_id="legs2", token_yes="Y", token_no="N",
        best_bid=0.70, best_ask=0.72,
    )
    alice = _user(session, "alice")
    # Update the position directly to hold 100 of the No token at entry 0.30.
    update_position_on_fill(session, alice, mkt, "N", 100, 0.30, "buy")
    session.commit()
    nw = compute_net_worth(session, alice)
    # No leg mark = 1 - 0.71 = 0.29 -> 100 * 0.29 = 29.0 (not 100 * 0.71 = 71.0)
    assert nw["positions_value"] == pytest.approx(29.0)


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


def test_force_resolve_no_pays_the_no_leg(session, market, starting_balance):
    """The No leg pays $1/share when the market resolves 'no'."""
    alice = _user(session, "alice")
    update_position_on_fill(session, alice, market, TOKEN_NO, 100, 0.30, "buy")
    session.commit()

    force_resolve_market(session, market.id, resolution="no")
    assert _balance(session, alice) == pytest.approx(starting_balance + 100.0)
    pos = get_or_create_position(session, alice, market, TOKEN_NO)
    assert float(pos.size) == pytest.approx(0.0)


def test_force_resolve_debits_short_the_full_payout(session, market, starting_balance):
    """A short on the winning leg owes $1/share at settlement.

    Regression: settlement used to skip negative positions, so a naked seller
    kept the sale proceeds and its liability vanished — minting paper money at
    resolution. Cash across both parties must be conserved.
    """
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    # Maker sells 10 Yes it doesn't hold at 0.30; taker crosses -> maker short 10.
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)

    result = force_resolve_market(session, market.id, resolution="yes")
    assert result["positions_settled"] == 2
    # Maker: +$3 sale proceeds, -$10 owed on the short = starting - 7.
    assert _balance(session, maker) == pytest.approx(starting_balance - 7.0)
    # Taker: paid $3 for 10 shares that pay $10 = starting + 7.
    assert _balance(session, taker) == pytest.approx(starting_balance + 7.0)
    # Zero-sum: settlement moved cash between members without minting any.
    assert _balance(session, maker) + _balance(session, taker) == pytest.approx(
        2 * starting_balance
    )
    pos = get_or_create_position(session, maker, market, TOKEN_YES)
    assert float(pos.size) == pytest.approx(0.0)


def test_force_resolve_short_on_losing_leg_owes_nothing(session, market, starting_balance):
    """A short on the losing leg keeps the sale proceeds and owes nothing."""
    maker = _user(session, "maker")
    taker = _user(session, "taker")
    place_paper_order(session, maker, market.id, TOKEN_YES, side="sell", price=0.30, size=10)
    place_paper_order(session, taker, market.id, TOKEN_YES, side="buy", price=0.50, size=10)

    force_resolve_market(session, market.id, resolution="no")
    # Maker keeps the $3; the shorted Yes shares expired worthless.
    assert _balance(session, maker) == pytest.approx(starting_balance + 3.0)
    # Taker's 10 Yes shares pay nothing.
    assert _balance(session, taker) == pytest.approx(starting_balance - 3.0)
    pos = get_or_create_position(session, maker, market, TOKEN_YES)
    assert float(pos.size) == pytest.approx(0.0)
