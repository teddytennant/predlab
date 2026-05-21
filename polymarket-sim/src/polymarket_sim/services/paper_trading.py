"""
Core paper trading service for polymarket-sim (Phase 2 Fidelity).

Responsibilities:
- Balance checks + escrow on order placement (buy notional reserved from cash)
- Position management (long only per outcome token for MVP)
- Fill processing: create Trade rows, update Positions, settle cash vs shares
- Mark-to-market P&L calculation using latest Market prices (best mid or last)
- Simple resolution settlement (manual force + hook for auto on sync when closed)
- Helpers for user queries (open orders, positions with unrealized P&L)

All amounts use float for simplicity in MVP (Numeric in DB but we cast). Production would
be more careful with Decimal everywhere.

The service is the single source of truth for "what happens to paper money on a fill".
OrderBook engine only does matching; this service owns the accounting.
"""

from __future__ import annotations

import logging
from datetime import datetime
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from ..models.db import Market, Order, PaperAccount, Position, Trade, User
from .auth import ensure_paper_account
from .orderbook import OrderBookEntry, get_orderbook

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _get_market_by_token(db: Session, token_id: str) -> Market | None:
    """Find the Market that owns this clob token id (scans JSON list - fine for small N)."""
    markets = db.execute(select(Market)).scalars().all()
    for m in markets:
        if m.clob_token_ids and token_id in m.clob_token_ids:
            return m
    return None


def _mid_from_market(m: Market) -> float:
    """Best available mid price from synced market data."""
    if m.best_bid is not None and m.best_ask is not None:
        return (float(m.best_bid) + float(m.best_ask)) / 2.0
    if m.last_trade_price is not None:
        return float(m.last_trade_price)
    if m.outcome_prices:
        try:
            return float(m.outcome_prices[0])
        except Exception:
            pass
    return 0.5


def _current_price_for_token(db: Session, token_id: str) -> float:
    m = _get_market_by_token(db, token_id)
    if m:
        return _mid_from_market(m)
    return 0.5


# ---------------------------------------------------------------------------
# Balance / Escrow
# ---------------------------------------------------------------------------


def get_available_balance(db: Session, user: User) -> float:
    """Free cash balance (does not subtract open buy escrows for ultra-simple MVP)."""
    acct = ensure_paper_account(db, user)
    return float(acct.balance_usd)


def _adjust_balance(db: Session, user: User, delta: float, reason: str) -> PaperAccount:
    """Internal: mutate paper balance. delta positive = credit."""
    acct = ensure_paper_account(db, user)
    # Numeric columns load back as Decimal; coerce to float so callers can pass
    # either a Decimal (reloaded order.price/size) or a float without TypeErrors.
    new_bal = float(acct.balance_usd) + float(delta)
    if new_bal < -1e-9:
        raise ValueError(f"Insufficient paper balance for {reason} (would go to {new_bal})")
    acct.balance_usd = new_bal  # type: ignore[assignment]
    logger.info(
        "Paper balance %+.2f for user=%s (%s). New bal=%.2f", delta, user.username, reason, new_bal
    )
    return acct


# ---------------------------------------------------------------------------
# Position helpers
# ---------------------------------------------------------------------------


def get_or_create_position(db: Session, user: User, market: Market, clob_token_id: str) -> Position:
    pos = db.execute(
        select(Position).where(
            Position.user_id == user.id,
            Position.market_id == market.id,
            Position.clob_token_id == clob_token_id,
        )
    ).scalar_one_or_none()
    if pos:
        return pos
    pos = Position(
        user_id=user.id,
        market_id=market.id,
        clob_token_id=clob_token_id,
        size=0.0,
        avg_entry_price=None,
    )
    db.add(pos)
    db.flush()
    return pos


def update_position_on_fill(
    db: Session,
    user: User,
    market: Market,
    clob_token_id: str,
    fill_size: float,
    fill_price: float,
    side: str,  # "buy" or "sell" from the order's perspective
) -> Position:
    """
    Update (or create) position after a successful fill.
    - buy: increase size, update VWAP avg_entry
    - sell: decrease size (allow going to 0, not negative for MVP)
    """
    pos = get_or_create_position(db, user, market, clob_token_id)
    old_size = pos.size
    old_avg = pos.avg_entry_price or fill_price

    if side == "buy":
        # New average entry
        total_cost_old = old_size * (old_avg or 0)
        total_cost_new = fill_size * fill_price
        new_size = old_size + fill_size
        new_avg = (total_cost_old + total_cost_new) / new_size if new_size > 0 else fill_price
        pos.size = new_size
        pos.avg_entry_price = new_avg
    else:  # sell
        new_size = old_size - fill_size
        if new_size < -1e-9:
            # For educational MVP we clamp; in reality this would be short or error
            logger.warning(
                "Sell would make position negative for %s on %s - clamping to 0",
                user.username,
                clob_token_id,
            )
            new_size = 0.0
        pos.size = max(0.0, new_size)
        if pos.size == 0:
            pos.avg_entry_price = None

    pos.updated_at = datetime.utcnow()
    logger.info(
        "Position updated user=%s token=%s %+.2f@%.4f -> size=%.2f avg=%.4f",
        user.username,
        clob_token_id[:8],
        fill_size if side == "buy" else -fill_size,
        fill_price,
        pos.size,
        pos.avg_entry_price or 0,
    )
    return pos


# ---------------------------------------------------------------------------
# Order placement with paper checks
# ---------------------------------------------------------------------------


def place_paper_order(
    db: Session,
    user: User,
    market_id: str,
    clob_token_id: str,
    side: str,
    price: float | None,
    size: float,
    order_type: str = "limit",
) -> Order:
    """
    High-level place: performs paper checks, persists Order, feeds the in-memory book,
    processes any immediate fills via the paper accounting rules.
    Returns the persisted (and possibly partially filled) Order.
    """
    market = db.get(Market, market_id)
    if not market:
        raise ValueError("Market not found")

    # Resolve token if not provided
    if not clob_token_id:
        if market.clob_token_ids:
            clob_token_id = market.clob_token_ids[0]
        else:
            raise ValueError("No clob_token_id and market has none")

    # Basic validation
    if size <= 0:
        raise ValueError("size must be > 0")
    if side not in ("buy", "sell"):
        raise ValueError("side must be buy or sell")

    is_buy = side == "buy"
    is_limit = price is not None
    exec_price = (
        float(price)
        if is_limit and price is not None
        else _current_price_for_token(db, clob_token_id)
    )

    # Paper checks on placement
    acct = ensure_paper_account(db, user)

    if is_buy:
        # For limit buy: escrow full notional now (simple conservative model)
        notional = exec_price * size
        if float(acct.balance_usd) < notional - 1e-9:
            raise ValueError(
                f"Insufficient paper balance to place buy order: need ~{notional:.2f}, have {acct.balance_usd:.2f}"
            )
        # Escrow: deduct now
        _adjust_balance(db, user, -notional, f"escrow buy {size}@{exec_price}")
    else:
        # Sell: must have position (or allow 0 for demo)
        pos = get_or_create_position(db, user, market, clob_token_id)
        if pos.size < size - 1e-9:
            logger.info("Sell order placed with insufficient current position (demo allowed)")

    # Persist the order (paper)
    order = Order(
        user_id=user.id,
        market_id=market_id,
        clob_token_id=clob_token_id,
        side=side,
        order_type=order_type,
        price=price,
        size=size,
        filled_size=0.0,
        status="open",
    )
    db.add(order)
    db.flush()  # get id for book

    # Feed the matching engine
    book = get_orderbook(clob_token_id)
    entry = OrderBookEntry(
        id=order.id,
        user_id=user.id,
        price=exec_price,
        size=size,
        side=side,
    )

    # For market orders we pass a very high (buy) or low (sell) price so it crosses everything
    if not is_limit:
        entry.price = 999.0 if is_buy else 0.0

    fills = book.add_limit_order(entry)  # works for both; market uses extreme price

    # Process fills immediately (accounting)
    total_filled_now = 0.0
    for f in fills:
        f_price = f["price"]
        f_size = f["size"]
        f_side = side  # our side

        # Record Trade
        trade = Trade(
            order_id=order.id,
            user_id=user.id,
            market_id=market_id,
            clob_token_id=clob_token_id,
            price=f_price,
            size=f_size,
            side=f_side,
        )
        db.add(trade)

        # Accounting
        _process_fill_accounting(db, user, market, clob_token_id, f_size, f_price, f_side, order)

        total_filled_now += f_size

    # Update order state
    order.filled_size = min(order.size, order.filled_size + total_filled_now)
    if order.filled_size >= order.size - 1e-9:
        order.status = "filled"
    elif order.filled_size > 0:
        order.status = "partial"
    else:
        order.status = "open"

    # If buy limit and not fully filled, the escrow for remaining is already deducted
    # (we only deducted full at place; on partial we don't refund partial escrow here - kept simple)

    db.commit()
    db.refresh(order)
    logger.info(
        "Paper order placed id=%s status=%s filled=%.2f", order.id, order.status, order.filled_size
    )
    return order


def _process_fill_accounting(
    db: Session,
    user: User,
    market: Market,
    clob_token_id: str,
    fill_size: float,
    fill_price: float,
    side: str,
    order: Order,
) -> None:
    """
    Core double-entry for paper:
    - BUY fill: position up, cash already escrowed so no further deduction (or adjust)
    - SELL fill: position down, cash credit
    """
    if side == "buy":
        # Position increases (we already escrowed the notional at place time for limits)
        update_position_on_fill(db, user, market, clob_token_id, fill_size, fill_price, "buy")
        # For market orders we deduct here instead
        if order.order_type == "market":
            notional = fill_price * fill_size
            _adjust_balance(db, user, -notional, f"market buy fill {fill_size}@{fill_price}")
    else:
        # SELL: credit cash, reduce position
        credit = fill_price * fill_size
        _adjust_balance(db, user, credit, f"sell fill {fill_size}@{fill_price}")
        update_position_on_fill(db, user, market, clob_token_id, fill_size, fill_price, "sell")


# ---------------------------------------------------------------------------
# Cancel (refund escrow for buys)
# ---------------------------------------------------------------------------


def cancel_paper_order(db: Session, user: User, order_id: int) -> Order:
    order = db.get(Order, order_id)
    if not order or order.user_id != user.id:
        raise ValueError("Order not found or not owned by user")
    if order.status not in ("open", "partial"):
        return order  # idempotent

    remaining = order.size - order.filled_size
    if remaining > 0 and order.side == "buy" and order.price is not None:
        refund = order.price * remaining
        _adjust_balance(db, user, refund, f"cancel refund buy order {order_id}")

    order.status = "cancelled"
    # Note: best-effort removal from live book omitted for MVP (simple list book);
    # the resting entry will eventually age out or be ignored on next hydrate.
    # Future: add remove_resting(id) to OrderBook.

    db.commit()
    db.refresh(order)
    logger.info("Paper order %s cancelled (refunded if buy)", order_id)
    return order


# ---------------------------------------------------------------------------
# User queries
# ---------------------------------------------------------------------------


def list_user_open_orders(db: Session, user: User) -> list[Order]:
    stmt = (
        select(Order)
        .where(Order.user_id == user.id, Order.status.in_(["open", "partial"]))
        .order_by(Order.created_at.desc())
    )
    return list(db.execute(stmt).scalars().all())


def list_user_positions_with_pnl(db: Session, user: User) -> list[dict[str, Any]]:
    """
    Return positions + mark-to-market unrealized P&L.
    """
    stmt = select(Position).where(Position.user_id == user.id)
    positions = db.execute(stmt).scalars().all()
    out: list[dict[str, Any]] = []
    for p in positions:
        market = db.get(Market, p.market_id)
        current_price = _current_price_for_token(db, p.clob_token_id) if market else 0.5
        avg = float(p.avg_entry_price or current_price)
        sz = float(p.size)
        unrealized = (current_price - avg) * sz
        out.append(
            {
                "market_id": p.market_id,
                "clob_token_id": p.clob_token_id,
                "size": p.size,
                "avg_entry_price": avg,
                "current_price": current_price,
                "unrealized_pnl": round(unrealized, 4),
                "market_question": market.question if market else "unknown",
            }
        )
    return out


# ---------------------------------------------------------------------------
# Leaderboard / net worth
# ---------------------------------------------------------------------------


def compute_net_worth(db: Session, user: User) -> dict[str, float]:
    """Total paper net worth = free cash + open positions marked to current price."""
    cash = float(ensure_paper_account(db, user).balance_usd)
    positions = db.execute(select(Position).where(Position.user_id == user.id)).scalars().all()
    pos_value = sum(float(p.size) * _current_price_for_token(db, p.clob_token_id) for p in positions)
    return {
        "cash": round(cash, 2),
        "positions_value": round(pos_value, 2),
        "net_worth": round(cash + pos_value, 2),
    }


def leaderboard(db: Session) -> list[dict[str, Any]]:
    """All users ranked by paper net worth, highest first."""
    users = db.execute(select(User)).scalars().all()
    rows = [{"username": u.username, "role": u.role, **compute_net_worth(db, u)} for u in users]
    rows.sort(key=lambda r: r["net_worth"], reverse=True)
    return rows


# ---------------------------------------------------------------------------
# Resolution / Settlement (simple)
# ---------------------------------------------------------------------------


def force_resolve_market(db: Session, market_id: str, resolution: str = "yes") -> dict[str, Any]:
    """
    Manual admin settlement for teaching.
    resolution: "yes" or "no" (maps to which leg pays 1.0)
    Credits winners 1.0 per share, losers 0, closes positions, adds to balance.
    """
    market = db.get(Market, market_id)
    if not market:
        raise ValueError("Market not found")

    # Determine payout per leg
    yes_payout = 1.0 if resolution.lower() == "yes" else 0.0
    no_payout = 1.0 - yes_payout

    # Find positions for this market
    stmt = select(Position).where(Position.market_id == market_id)
    positions = db.execute(stmt).scalars().all()

    settled = 0
    total_payout = 0.0
    for pos in positions:
        if pos.size <= 0:
            continue
        # Which leg?
        # Heuristic: first token in market is Yes, second No (common convention)
        token_list = market.clob_token_ids or []
        payout = yes_payout if (not token_list or pos.clob_token_id == token_list[0]) else no_payout
        proceeds = float(pos.size) * payout
        user = db.get(User, pos.user_id)
        if user:
            _adjust_balance(db, user, proceeds, f"resolution payout {market_id} {resolution}")
        # Close position
        pos.size = 0.0
        pos.avg_entry_price = None
        settled += 1
        total_payout += proceeds

    # Mark market closed
    market.closed = True
    market.active = False
    db.commit()

    logger.info(
        "Force resolved market %s as %s: %d positions settled, $%.2f paid",
        market_id,
        resolution,
        settled,
        total_payout,
    )
    return {
        "market_id": market_id,
        "resolution": resolution,
        "positions_settled": settled,
        "total_payout": total_payout,
    }


# Auto-detect hook (called from sync after upsert if market now closed)
def maybe_auto_settle_on_sync(db: Session, market: Market) -> None:
    if market.closed and not market.active:
        # If already has positions, we could settle at 1.0 / 0.0 based on last known outcomePrices
        # For MVP: only log; real auto would look at resolved outcome from gamma (future field)
        logger.info(
            "Market %s closed in upstream - admin can call force_resolve for settlement demo",
            market.id,
        )
