"""
Simple in-memory order book: one book per outcome token (clob_token_id),
price-time priority matching for limit orders.

This is the matching core of the paper trading CLOB simulator; all cash and
position accounting lives in the paper_trading service.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from ..util import utcnow


@dataclass
class OrderBookEntry:
    """A single resting order in the book."""

    id: int  # our DB order id
    user_id: int
    price: float
    size: float  # remaining
    side: str  # "buy" or "sell"
    created_at: datetime = field(default_factory=utcnow)


@dataclass
class OrderBook:
    """In-memory order book for one outcome leg (one clob token id).

    Bids: highest price first (max-heap like, we sort on access)
    Asks: lowest price first
    """

    token_id: str
    bids: list[OrderBookEntry] = field(default_factory=list)  # buys, sorted desc price
    asks: list[OrderBookEntry] = field(default_factory=list)  # sells, sorted asc price

    def _sort(self) -> None:
        self.bids.sort(key=lambda e: (-e.price, e.created_at))
        self.asks.sort(key=lambda e: (e.price, e.created_at))

    def add_limit_order(self, entry: OrderBookEntry) -> list[dict[str, Any]]:
        """Add a limit order and attempt immediate matching. Returns list of fills."""
        self._sort()
        fills: list[dict[str, Any]] = []

        if entry.side == "buy":
            # Match against asks
            remaining = entry.size
            i = 0
            while i < len(self.asks) and remaining > 0:
                ask = self.asks[i]
                if ask.price > entry.price:
                    break  # no cross
                fill_qty = min(remaining, ask.size)
                fills.append(
                    {
                        "price": ask.price,
                        "size": fill_qty,
                        "side": "buy",
                        "counter_order_id": ask.id,
                    }
                )
                ask.size -= fill_qty
                remaining -= fill_qty
                if ask.size <= 0:
                    self.asks.pop(i)
                else:
                    i += 1
            if remaining > 0:
                entry.size = remaining
                self.bids.append(entry)
        else:  # sell
            remaining = entry.size
            i = 0
            while i < len(self.bids) and remaining > 0:
                bid = self.bids[i]
                if bid.price < entry.price:
                    break
                fill_qty = min(remaining, bid.size)
                fills.append(
                    {
                        "price": bid.price,
                        "size": fill_qty,
                        "side": "sell",
                        "counter_order_id": bid.id,
                    }
                )
                bid.size -= fill_qty
                remaining -= fill_qty
                if bid.size <= 0:
                    self.bids.pop(i)
                else:
                    i += 1
            if remaining > 0:
                entry.size = remaining
                self.asks.append(entry)

        self._sort()
        return fills

    def snapshot(self) -> dict[str, Any]:
        """Return current book snapshot (top of book etc)."""
        self._sort()
        return {
            "token_id": self.token_id,
            "bids": [{"price": e.price, "size": e.size, "order_id": e.id} for e in self.bids[:10]],
            "asks": [{"price": e.price, "size": e.size, "order_id": e.id} for e in self.asks[:10]],
        }


# Global in-memory books (keyed by token id).
_order_books: dict[str, OrderBook] = {}


def get_orderbook(token_id: str) -> OrderBook:
    if token_id not in _order_books:
        _order_books[token_id] = OrderBook(token_id=token_id)
    return _order_books[token_id]


def restore_resting_order(token_id: str, entry: OrderBookEntry) -> None:
    """Re-insert a persisted open order as a resting entry (startup hydration).

    No matching is attempted — this rebuilds the in-memory books from DB rows
    after a restart, and those orders already had their chance to cross.
    """
    book = get_orderbook(token_id)
    if entry.side == "buy":
        book.bids.append(entry)
    else:
        book.asks.append(entry)
    book._sort()


def reset_orderbook(token_id: str | None = None) -> None:
    """For tests / admin resets."""
    global _order_books
    if token_id:
        _order_books.pop(token_id, None)
    else:
        _order_books.clear()


def remove_resting_order(order_id: int) -> bool:
    """Drop a single resting entry (by DB order id) from whichever book holds it.

    The book is in-memory and outlives the DB status change, so a cancelled order
    left here would keep matching incoming orders — handing the canceller a fill
    *after* their escrow was already refunded (a double credit). Returns True if an
    entry was removed.
    """
    for book in _order_books.values():
        before = len(book.bids) + len(book.asks)
        book.bids = [e for e in book.bids if e.id != order_id]
        book.asks = [e for e in book.asks if e.id != order_id]
        if len(book.bids) + len(book.asks) < before:
            return True
    return False


def remove_user_orders(user_id: int) -> int:
    """Drop all of a user's resting entries from every book (admin reset/delete).

    The book is in-memory and survives a DB row delete, so we must purge it too
    or the user's cancelled/deleted orders would keep matching. Returns the count
    of entries removed.
    """
    removed = 0
    for book in _order_books.values():
        kept_bids = [e for e in book.bids if e.user_id != user_id]
        kept_asks = [e for e in book.asks if e.user_id != user_id]
        removed += (len(book.bids) - len(kept_bids)) + (len(book.asks) - len(kept_asks))
        book.bids, book.asks = kept_bids, kept_asks
    return removed
