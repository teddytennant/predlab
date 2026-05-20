"""Simple in-memory order book for Kalshi binary markets (Phase 1 stub).

Kalshi markets are binary (yes/no). The public orderbook endpoint returns
yes bids/asks (and implicitly the complementary no side).

This is a price-time priority stub. Full matching + position updates happen
in paper_trading service (Phase 2).

For foundation we just maintain resting orders in memory and provide
a snapshot that the /orderbook endpoint can return.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

from sqlalchemy.orm import Session as SASession

from ..models.db import Order as DBOrder  # type: ignore


@dataclass
class RestingOrder:
    order_id: str
    user_id: str
    side: Literal["yes", "no"]
    action: Literal["buy", "sell"]
    price_dollars: str  # "0.65"
    remaining_count: int
    created_ts: float  # monotonic for time priority


@dataclass
class OrderBook:
    """Per-market orderbook.

    Kalshi public orderbook returns "orderbook_fp" with yes_dollars and no_dollars
    arrays of [price_str, size_str] pairs. yes_dollars = bids to buy YES.
    no_dollars = bids to buy NO (== asks for YES at 1-p).
    """

    ticker: str
    yes_bids: list[RestingOrder] = field(default_factory=list)  # buy yes (bid)
    no_bids: list[RestingOrder] = field(default_factory=list)   # buy no == sell yes (ask)

    def snapshot(self) -> dict:
        """Return *exact* Kalshi public orderbook shape:
        {"orderbook_fp": {"yes_dollars": [[price_str, size_str], ...], "no_dollars": [...] } }
        Sorted ascending price; best (highest for bids) is last per Kalshi convention.
        """
        # yes_bids: buy YES, sort ascending price for the array (last = best/highest bid)
        yes_bid_levels: dict[str, int] = {}
        for o in self.yes_bids:
            p = o.price_dollars
            yes_bid_levels[p] = yes_bid_levels.get(p, 0) + o.remaining_count

        # no_bids: buy NO == sell YES, same
        no_bid_levels: dict[str, int] = {}
        for o in self.no_bids:
            p = o.price_dollars
            no_bid_levels[p] = no_bid_levels.get(p, 0) + o.remaining_count

        def to_pairs(levels_map: dict[str, int]) -> list[list[str]]:
            # ascending price
            items = sorted(levels_map.items(), key=lambda kv: float(kv[0]))
            return [[p, str(s)] for p, s in items]

        return {
            "orderbook_fp": {
                "yes_dollars": to_pairs(yes_bid_levels),
                "no_dollars": to_pairs(no_bid_levels),
            },
            "simulated": True,
        }


# Global in-memory registry for Phase 1 (later move to Redis keyed by ticker)
_orderbooks: dict[str, OrderBook] = {}


def get_orderbook(ticker: str) -> OrderBook:
    if ticker not in _orderbooks:
        _orderbooks[ticker] = OrderBook(ticker=ticker)
    return _orderbooks[ticker]


def add_resting_order(
    ticker: str,
    order_id: str,
    user_id: str,
    side: Literal["yes", "no", "bid", "ask"],  # V2 bid/ask or legacy
    action: Literal["buy", "sell"],
    price_dollars: str,
    count: int,
) -> RestingOrder:
    """Add to the appropriate side of the book.
    In V2 terms: bid -> yes_bids (buy YES), ask -> no_bids (sell YES = buy NO)
    """
    ob = get_orderbook(ticker)
    import time

    # Normalize V2 side to internal
    is_buy_yes = False
    if side in ("bid", "yes") or (side == "yes" and action == "buy"):
        is_buy_yes = True
    # else: ask / sell yes -> no_bids

    ro = RestingOrder(
        order_id=order_id,
        user_id=user_id,
        side=side if side in ("bid", "ask") else ("yes" if is_buy_yes else "no"),
        action=action,
        price_dollars=price_dollars,
        remaining_count=count,
        created_ts=time.monotonic(),
    )
    if is_buy_yes:
        ob.yes_bids.append(ro)
    else:
        ob.no_bids.append(ro)
    return ro


def remove_order(ticker: str, order_id: str) -> bool:
    """Cancel helper (removes from in-mem book)."""
    ob = get_orderbook(ticker)
    for lst in (ob.yes_bids, ob.no_bids):
        for i, o in enumerate(lst):
            if o.order_id == order_id:
                del lst[i]
                return True
    return False


# --- Rebuild & helpers for Phase 2 persistence integration ---


def rebuild_books_from_db(db: SASession) -> int:
    """On startup (or after bulk ops), load open DB orders into in-memory books.
    This makes the live orderbook reflect persisted resting orders.
    Returns number of orders loaded.
    """
    import time as _time

    # Clear current in-mem (fresh)
    global _orderbooks
    _orderbooks.clear()

    open_orders = (
        db.query(DBOrder)
        .filter(DBOrder.status == "open")
        .order_by(DBOrder.created_at)
        .all()
    )

    count = 0
    for o in open_orders:
        # Reconstruct side
        side = o.side
        action = o.action or "buy"
        ob = get_orderbook(o.ticker)
        ro = RestingOrder(
            order_id=o.id,
            user_id=o.user_id,
            side=side,
            action=action,
            price_dollars=o.price_dollars,
            remaining_count=o.count - o.filled_count,
            created_ts=_time.mktime(o.created_at.timetuple()) if o.created_at else _time.monotonic(),
        )
        if side in ("bid", "yes"):
            ob.yes_bids.append(ro)
        else:
            ob.no_bids.append(ro)
        count += 1

    return count


def get_user_open_orders(ticker: str | None, user_id: str) -> list[RestingOrder]:
    """Return in-mem resting for a user (used for self-trade checks, cancel all etc)."""
    results: list[RestingOrder] = []
    for ob in _orderbooks.values():
        if ticker and ob.ticker != ticker:
            continue
        for lst in (ob.yes_bids, ob.no_bids):
            for ro in lst:
                if ro.user_id == user_id:
                    results.append(ro)
    return results
