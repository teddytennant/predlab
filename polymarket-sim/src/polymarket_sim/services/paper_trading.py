"""
Core paper trading service for polymarket-sim (Phase 2 Fidelity).

Responsibilities:
- Balance checks + escrow on order placement (buy notional reserved from cash)
- Position management (long or short per outcome token; a sell past your holdings
  opens a short that compute_net_worth marks as a liability)
- Fill processing: create Trade rows, update Positions, settle cash vs shares
- Mark-to-market P&L calculation using latest Market prices (best mid or last)
- Resolution settlement via the manual admin force-resolve path
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

from sqlalchemy import delete, select, update
from sqlalchemy.orm import Session

from ..models.db import Market, NetWorthSnapshot, Order, PaperAccount, Position, Trade, User
from .auth import ensure_paper_account
from .orderbook import (
    OrderBookEntry,
    get_orderbook,
    remove_resting_order,
    remove_user_orders,
    reset_orderbook,
)

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
    """Per-share mark for one specific outcome token.

    ``Market.best_bid``/``best_ask`` are synced from Gamma's ``bestBid``/``bestAsk``,
    which describe the *first* (Yes) leg only. A binary market has two tokens
    whose prices sum to 1, so a position on the second (No) leg must be valued
    at ``1 - yes_mid`` — otherwise No-leg holdings are silently marked at the
    dominant Yes-leg price and reported P&L is inverted.
    """
    m = _get_market_by_token(db, token_id)
    if not m:
        return 0.5
    yes_mid = _mid_from_market(m)
    tokens = m.clob_token_ids or []
    if len(tokens) >= 2 and token_id == tokens[1]:
        return 1.0 - yes_mid
    return yes_mid


# ---------------------------------------------------------------------------
# Balance / Escrow
# ---------------------------------------------------------------------------


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
    - sell: decrease size; selling past your holdings drives size negative (a
      short), which compute_net_worth values as a liability at the current mark
    """
    pos = get_or_create_position(db, user, market, clob_token_id)
    # Numeric columns reload from the DB as Decimal; coerce so arithmetic with
    # the incoming float fill never raises (Decimal - float is a TypeError).
    old_size = float(pos.size or 0.0)
    old_avg = float(pos.avg_entry_price if pos.avg_entry_price is not None else fill_price)
    fill_size = float(fill_size)
    fill_price = float(fill_price)

    if side == "buy":
        # New average entry
        total_cost_old = old_size * (old_avg or 0)
        total_cost_new = fill_size * fill_price
        new_size = old_size + fill_size
        new_avg = (total_cost_old + total_cost_new) / new_size if new_size > 0 else fill_price
        pos.size = new_size
        pos.avg_entry_price = new_avg
    else:  # sell
        # Let the position go negative — a sell beyond holdings opens a short, which
        # compute_net_worth marks as a liability at the current price. Clamping to 0
        # here (the old MVP behaviour) credited the sale proceeds while silently
        # dropping the shares owed: pure money creation that inflated net worth.
        new_size = old_size - fill_size
        pos.size = 0.0 if abs(new_size) < 1e-9 else new_size
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
        # Sell: makers rest asks to seed the two-sided book, so a sell beyond
        # current holdings is allowed and opens a *short* (negative position). The
        # short is marked as a liability in compute_net_worth, so the sale proceeds
        # are offset and no paper money is minted (see update_position_on_fill).
        get_or_create_position(db, user, market, clob_token_id)

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

        # Accounting for the taker (incoming) order. The taker escrowed ``exec_price``
        # per share at placement (limit price, or the mark for a market order).
        _process_fill_accounting(db, user, market, clob_token_id, f_size, f_price, f_side, exec_price)
        # ...and the resting maker on the other side of the same fill.
        _settle_resting_counterparty(db, market, clob_token_id, f, f_side)

        total_filled_now += f_size

    # Update order state
    order.filled_size = min(order.size, order.filled_size + total_filled_now)
    if order.filled_size >= order.size - 1e-9:
        order.status = "filled"
    elif order.filled_size > 0:
        order.status = "partial"
    else:
        order.status = "open"

    # On a partial fill the filled portion was reconciled above; the escrow for the
    # remaining (resting) size stays reserved and is refunded when it cancels.

    # A fill moves the taker's net worth — drop a point on their curve.
    if total_filled_now > 0:
        record_snapshot(db, user)

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
    escrow_price: float,
) -> None:
    """
    Core double-entry for paper:
    - BUY fill: position up; cash was already escrowed at ``escrow_price`` when the
      order was placed, so here we only reconcile the difference against the actual
      ``fill_price``. A buy that crosses a cheaper resting ask (fill < escrow) is
      refunded the slack; a market buy that walks up the book (fill > escrow) is
      charged the extra. Net effect: cash + escrow + position value is conserved.
    - SELL fill: position down, cash credit (sells reserve no escrow).
    """
    if side == "buy":
        update_position_on_fill(db, user, market, clob_token_id, fill_size, fill_price, "buy")
        # Escrow reserved ``escrow_price`` per share at placement; settle vs the fill.
        # Positive delta => over-reserved, refund; negative => crossed up, charge more.
        delta = (escrow_price - fill_price) * fill_size
        if abs(delta) > 1e-9:
            _adjust_balance(db, user, delta, f"escrow reconcile buy fill {fill_size}@{fill_price}")
    else:
        # SELL: credit cash, reduce position
        credit = fill_price * fill_size
        _adjust_balance(db, user, credit, f"sell fill {fill_size}@{fill_price}")
        update_position_on_fill(db, user, market, clob_token_id, fill_size, fill_price, "sell")


def _settle_resting_counterparty(
    db: Session,
    market: Market,
    clob_token_id: str,
    fill: dict[str, Any],
    taker_side: str,
) -> None:
    """Settle the maker side of a fill so both parties' books move.

    The matching engine fills a taker against resting orders, but place only ran
    accounting for the incoming (taker) order — leaving the resting owner's cash
    and position untouched. Apply the mirror-image fill to the maker. A resting
    buy already escrowed its notional at placement, and a resting sell escrowed
    nothing, so ``_process_fill_accounting`` settles each correctly as-is.
    """
    counter_id = fill.get("counter_order_id")
    if counter_id is None:
        return
    maker_order = db.get(Order, counter_id)
    if maker_order is None:
        return
    if maker_order.status == "cancelled":
        # Defensive: a cancelled order should already be out of the book, but never
        # settle a fill against one — its escrow was refunded on cancel.
        return
    maker_user = db.get(User, maker_order.user_id)
    if maker_user is None:
        return

    f_price = float(fill["price"])
    f_size = float(fill["size"])
    maker_side = "sell" if taker_side == "buy" else "buy"
    # A resting buy maker escrowed at its own limit price; the fill settles against that.
    maker_escrow = float(maker_order.price) if maker_order.price is not None else f_price

    db.add(
        Trade(
            order_id=maker_order.id,
            user_id=maker_user.id,
            market_id=market.id,
            clob_token_id=clob_token_id,
            price=f_price,
            size=f_size,
            side=maker_side,
        )
    )
    _process_fill_accounting(db, maker_user, market, clob_token_id, f_size, f_price, maker_side, maker_escrow)

    filled = float(maker_order.filled_size or 0.0) + f_size
    maker_order.filled_size = min(float(maker_order.size), filled)
    maker_order.status = "filled" if filled >= float(maker_order.size) - 1e-9 else "partial"

    # The maker's net worth moved too — record their point on the same fill.
    record_snapshot(db, maker_user)


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
    # Purge the resting entry from the in-memory book so it can no longer match.
    # The book outlives this DB status change, so a cancelled-but-resting order
    # would otherwise still fill — double-crediting the canceller, who was just
    # refunded its escrow above.
    remove_resting_order(order_id)

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


def _escrow_price_for_buy(db: Session, order: Order) -> float:
    """Per-share cash a resting buy order reserved at placement.

    Limit buys reserve their limit price; a market buy (no limit price) reserved the
    mark, which we re-derive from current market data.
    """
    if order.price is not None:
        return float(order.price)
    if order.clob_token_id is None:
        return 0.5
    return _current_price_for_token(db, order.clob_token_id)


def compute_net_worth(db: Session, user: User) -> dict[str, float]:
    """Total paper net worth = free cash + escrow locked in resting buys + positions marked.

    Cash reserved in open buy orders was debited from the free balance at placement,
    so it must be counted here too — otherwise resting orders silently shrink net worth.
    """
    cash = float(ensure_paper_account(db, user).balance_usd)
    positions = db.execute(select(Position).where(Position.user_id == user.id)).scalars().all()
    pos_value = sum(float(p.size) * _current_price_for_token(db, p.clob_token_id) for p in positions)

    open_buys = db.execute(
        select(Order).where(
            Order.user_id == user.id,
            Order.side == "buy",
            Order.status.in_(["open", "partial"]),
        )
    ).scalars().all()
    open_orders_value = sum(
        _escrow_price_for_buy(db, o) * (float(o.size) - float(o.filled_size)) for o in open_buys
    )

    return {
        "cash": round(cash, 2),
        "positions_value": round(pos_value, 2),
        "open_orders_value": round(open_orders_value, 2),
        "net_worth": round(cash + pos_value + open_orders_value, 2),
    }


def leaderboard(db: Session) -> list[dict[str, Any]]:
    """All users ranked by paper net worth, highest first."""
    users = db.execute(select(User)).scalars().all()
    rows = [{"username": u.username, "role": u.role, **compute_net_worth(db, u)} for u in users]
    rows.sort(key=lambda r: r["net_worth"], reverse=True)
    return rows


# ---------------------------------------------------------------------------
# Net-worth history (for the per-user profile graph)
# ---------------------------------------------------------------------------


def record_snapshot(db: Session, user: User) -> NetWorthSnapshot:
    """Append one point to a user's net-worth-over-time curve.

    Cheap row; flushed (not committed) so callers control the transaction.
    The pre-flush ensures pending mutations in the same transaction (a reset,
    a fill, a cancel) are visible to ``compute_net_worth``'s SELECTs — the
    session uses ``autoflush=False`` so we must do it ourselves.
    """
    db.flush()
    nw = compute_net_worth(db, user)
    snap = NetWorthSnapshot(
        user_id=user.id,
        net_worth=nw["net_worth"],
        cash=nw["cash"],
        positions_value=nw["positions_value"],
    )
    db.add(snap)
    db.flush()
    return snap


def record_all_snapshots(db: Session) -> int:
    """Snapshot every user's net worth (periodic tick / settlement). Commits."""
    users = db.execute(select(User)).scalars().all()
    for u in users:
        record_snapshot(db, u)
    db.commit()
    return len(users)


def get_user_detail(
    db: Session, username: str, *, trade_limit: int = 100, snapshot_limit: int = 1000
) -> dict[str, Any] | None:
    """Everything the profile page needs: net worth, positions, trades, history.

    Returns ``None`` when the username does not exist.
    """
    user = db.execute(select(User).where(User.username == username)).scalar_one_or_none()
    if not user:
        return None

    trades = (
        db.execute(
            select(Trade)
            .where(Trade.user_id == user.id)
            .order_by(Trade.created_at.desc())
            .limit(trade_limit)
        )
        .scalars()
        .all()
    )
    snapshots = (
        db.execute(
            select(NetWorthSnapshot)
            .where(NetWorthSnapshot.user_id == user.id)
            .order_by(NetWorthSnapshot.created_at.asc())
            .limit(snapshot_limit)
        )
        .scalars()
        .all()
    )

    return {
        "username": user.username,
        "display_name": user.display_name,
        "role": user.role,
        **compute_net_worth(db, user),
        # ``list_user_positions_with_pnl`` returns ``size`` as Decimal (the raw
        # column type), which FastAPI serialises as a JSON string. Cast it so
        # the leaderboard's typed JSON deserialiser can read a number here.
        "positions": [
            {**p, "size": float(p["size"])}
            for p in list_user_positions_with_pnl(db, user)
        ],
        "trades": [
            {
                "id": t.id,
                "market_id": t.market_id,
                "clob_token_id": t.clob_token_id,
                "side": t.side,
                "price": float(t.price),
                "size": float(t.size),
                "created_at": t.created_at.isoformat(),
            }
            for t in trades
        ],
        "history": [
            {
                "t": s.created_at.isoformat(),
                "net_worth": float(s.net_worth),
                "cash": float(s.cash),
                "positions_value": float(s.positions_value),
            }
            for s in snapshots
        ],
    }


# ---------------------------------------------------------------------------
# Admin: balance resets & member removal (teaching ops)
# ---------------------------------------------------------------------------


def reset_user_to_starting(db: Session, user: User, starting_balance: float) -> dict[str, Any]:
    """Return one member to a clean starting state.

    Cancels open orders (and purges them from the live book), clears all
    positions, and restores cash to the starting balance. Net worth afterwards
    is exactly ``starting_balance``.
    """
    open_orders = (
        db.execute(
            select(Order).where(Order.user_id == user.id, Order.status.in_(["open", "partial"]))
        )
        .scalars()
        .all()
    )
    for o in open_orders:
        o.status = "cancelled"
    remove_user_orders(user.id)

    db.execute(delete(Position).where(Position.user_id == user.id))

    acct = ensure_paper_account(db, user)
    acct.balance_usd = starting_balance  # type: ignore[assignment]
    record_snapshot(db, user)
    db.commit()
    return {
        "reset": user.username,
        "balance": starting_balance,
        "orders_cancelled": len(open_orders),
    }


def reset_all_to_starting(db: Session, starting_balance: float) -> dict[str, Any]:
    """Reset every member to a clean starting state (start-of-competition wipe)."""
    users = db.execute(select(User)).scalars().all()
    for u in users:
        db.execute(
            update(Order)
            .where(Order.user_id == u.id, Order.status.in_(["open", "partial"]))
            .values(status="cancelled")
        )
    db.execute(delete(Position))
    for u in users:
        ensure_paper_account(db, u).balance_usd = starting_balance  # type: ignore[assignment]
    for u in users:
        record_snapshot(db, u)
    db.commit()
    reset_orderbook()  # clear every in-memory book at once
    return {"reset": "all", "count": len(users), "balance": starting_balance}


def delete_user(db: Session, username: str) -> dict[str, Any]:
    """Permanently remove a member and everything they own (they left the club).

    Deletes trades, orders and positions first (no cascade on those), then the
    user row — which cascades their API keys and paper account — and finally
    purges any resting orders from the in-memory book.
    """
    user = db.execute(select(User).where(User.username == username)).scalar_one_or_none()
    if not user:
        raise ValueError("user not found")
    user_id = user.id
    db.execute(delete(Trade).where(Trade.user_id == user_id))
    db.execute(delete(Order).where(Order.user_id == user_id))
    db.execute(delete(Position).where(Position.user_id == user_id))
    db.delete(user)  # cascades api_keys + paper_account
    db.commit()
    remove_user_orders(user_id)
    return {"deleted": username}


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

    # Settlement shifts winners' and losers' net worth — snapshot everyone.
    for u in db.execute(select(User)).scalars().all():
        record_snapshot(db, u)
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
