"""Unit tests for the in-memory CLOB matching engine (pure, no DB)."""

from __future__ import annotations

from polymarket_sim.services.orderbook import OrderBook, OrderBookEntry


def _entry(oid: int, side: str, price: float, size: float) -> OrderBookEntry:
    return OrderBookEntry(id=oid, user_id=oid, price=price, size=size, side=side)


def test_resting_buy_does_not_fill_empty_book():
    book = OrderBook(token_id="t")
    fills = book.add_limit_order(_entry(1, "buy", 0.50, 10))
    assert fills == []
    assert len(book.bids) == 1
    assert book.bids[0].size == 10


def test_no_cross_when_bid_below_ask():
    book = OrderBook(token_id="t")
    book.add_limit_order(_entry(1, "sell", 0.60, 10))
    fills = book.add_limit_order(_entry(2, "buy", 0.50, 10))
    assert fills == []
    assert len(book.asks) == 1
    assert len(book.bids) == 1


def test_buy_crosses_resting_ask_at_maker_price():
    book = OrderBook(token_id="t")
    book.add_limit_order(_entry(1, "sell", 0.40, 10))  # resting maker ask
    fills = book.add_limit_order(_entry(2, "buy", 0.50, 10))  # aggressive taker
    assert len(fills) == 1
    # Trade executes at the resting maker's price, not the taker's limit.
    assert fills[0]["price"] == 0.40
    assert fills[0]["size"] == 10
    assert book.asks == [] and book.bids == []  # both fully consumed


def test_partial_fill_leaves_remainder_resting():
    book = OrderBook(token_id="t")
    book.add_limit_order(_entry(1, "sell", 0.40, 4))
    fills = book.add_limit_order(_entry(2, "buy", 0.50, 10))
    assert sum(f["size"] for f in fills) == 4
    # 6 unfilled units rest as a bid.
    assert len(book.bids) == 1
    assert book.bids[0].size == 6


def test_price_priority_fills_cheapest_ask_first():
    book = OrderBook(token_id="t")
    book.add_limit_order(_entry(1, "sell", 0.45, 5))
    book.add_limit_order(_entry(2, "sell", 0.40, 5))  # cheaper -> should fill first
    fills = book.add_limit_order(_entry(3, "buy", 0.50, 5))
    assert len(fills) == 1
    assert fills[0]["price"] == 0.40
    assert fills[0]["counter_order_id"] == 2


def test_sell_crosses_resting_bid():
    book = OrderBook(token_id="t")
    book.add_limit_order(_entry(1, "buy", 0.60, 10))
    fills = book.add_limit_order(_entry(2, "sell", 0.50, 10))
    assert len(fills) == 1
    assert fills[0]["price"] == 0.60  # maker bid price
    assert fills[0]["side"] == "sell"
