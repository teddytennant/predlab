"""Core paper trading engine for Kalshi-sim Phase 2.

Responsibilities:
- Real RSA-authenticated order placement via V2 shape
- Limit order matching (price-time priority) against in-memory books + persisted orders
- Balance (cents) updates, Position tracking (signed yes exposure)
- Trade/Fill recording
- Basic TIF handling (GTC, IOC, FOK)
- Self-trade prevention
- Settlement on resolution (admin or future sync detect)
- User-scoped queries for positions, fills, balance, open orders

All money in cents. Contracts as whole ints for MVP (fp strings at API boundary).
"""

from __future__ import annotations

import logging
import time as _time
from decimal import Decimal
from typing import Optional
from uuid import uuid4

from sqlalchemy.orm import Session

from ..config import Settings, get_settings
from ..models.api import (
    CreateOrderRequestV2,
    CreateOrderResponseV2,
    FillResponse,
    GetBalanceResponse,
    GetFillsResponse,
    GetOrdersResponse,
    GetPositionsResponse,
    MarketPosition,
    OrderSummary,
)
from ..models.db import Market, PaperAccount, Position, Trade, User
from ..models.db import Order as DBOrder
from ..utils.fp import to_count_fp, to_dollars_fp
from .orderbook import (
    add_resting_order,
    get_orderbook,
    rebuild_books_from_db,
    remove_order,
)

logger = logging.getLogger(__name__)


def _price_to_cents(price_str: str) -> int:
    """0.6500 -> 65"""
    d = Decimal(price_str)
    return int((d * Decimal("100")).to_integral_value(rounding="ROUND_HALF_UP"))


def _cents_to_price_str(cents: int) -> str:
    d = Decimal(cents) / Decimal("100")
    return to_dollars_fp(d, 4)


def _count_to_int(count_str: str) -> int:
    """ "10.00" -> 10 (MVP: whole contracts; fractional later) """
    d = Decimal(count_str)
    return int(d.to_integral_value(rounding="ROUND_DOWN"))


class PaperTradingService:
    """Stateless-ish service (per request db session)."""

    def __init__(self, db: Session, settings: Optional[Settings] = None):
        self.db = db
        self.settings = settings or get_settings()

    def _get_or_create_paper_account(self, user_id: str) -> PaperAccount:
        pa = (
            self.db.query(PaperAccount)
            .filter(PaperAccount.user_id == user_id)
            .first()
        )
        if not pa:
            pa = PaperAccount(
                user_id=user_id,
                balance_cents=self.settings.starting_balance_cents,
            )
            self.db.add(pa)
            self.db.commit()
            self.db.refresh(pa)
        return pa

    def _get_or_create_position(self, user_id: str, ticker: str) -> Position:
        pos = (
            self.db.query(Position)
            .filter(Position.user_id == user_id, Position.ticker == ticker)
            .first()
        )
        if not pos:
            pos = Position(user_id=user_id, ticker=ticker, yes_contracts=0)
            self.db.add(pos)
            self.db.commit()
            self.db.refresh(pos)
        return pos

    def get_balance(self, user_id: str) -> GetBalanceResponse:
        pa = self._get_or_create_paper_account(user_id)
        # Naive portfolio_value: use last_price * positions (very rough)
        positions = self.db.query(Position).filter(Position.user_id == user_id).all()
        port_value_cents = 0
        for p in positions:
            m = self.db.get(Market, p.ticker)
            last = Decimal(m.last_price_dollars) if m else Decimal("0.5")
            # signed exposure value at current last
            port_value_cents += int(Decimal(p.yes_contracts) * last * Decimal("100"))
        ts = int(_time.time())
        return GetBalanceResponse(
            balance=pa.balance_cents,
            balance_dollars=_cents_to_price_str(pa.balance_cents),
            portfolio_value=port_value_cents,
            updated_ts=ts,
        )

    def get_positions(self, user_id: str) -> GetPositionsResponse:
        positions = self.db.query(Position).filter(Position.user_id == user_id).all()
        mp = []
        open_orders = self.db.query(DBOrder).filter(
            DBOrder.user_id == user_id, DBOrder.status.in_(["open", "partially_filled"])
        ).all()
        per_ticker_resting = {}
        for o in open_orders:
            per_ticker_resting[o.ticker] = per_ticker_resting.get(o.ticker, 0) + 1

        for p in positions:
            mp.append(
                MarketPosition(
                    ticker=p.ticker,
                    position_fp=to_count_fp(p.yes_contracts),
                    resting_orders_count=per_ticker_resting.get(p.ticker, 0),
                    last_updated_ts=p.updated_at.isoformat() if p.updated_at else None,
                )
            )
        return GetPositionsResponse(market_positions=mp)

    def get_fills(self, user_id: str, limit: int = 50) -> GetFillsResponse:
        trades = (
            self.db.query(Trade)
            .filter(Trade.user_id == user_id)
            .order_by(Trade.created_at.desc())
            .limit(limit)
            .all()
        )
        fills = []
        for t in trades:
            fills.append(
                FillResponse(
                    fill_id=t.id,
                    order_id=t.order_id,
                    ticker=t.ticker,
                    count_fp=to_count_fp(t.count),
                    yes_price_dollars=t.price_dollars,
                    created_time=t.created_at.isoformat() if t.created_at else "",
                )
            )
        return GetFillsResponse(fills=fills)

    def get_orders(self, user_id: str, status: str | None = None) -> GetOrdersResponse:
        q = self.db.query(DBOrder).filter(DBOrder.user_id == user_id)
        if status:
            q = q.filter(DBOrder.status == status)
        orders = q.order_by(DBOrder.created_at.desc()).limit(100).all()
        summaries = []
        for o in orders:
            summaries.append(
                OrderSummary(
                    order_id=o.id,
                    client_order_id=o.client_order_id,
                    ticker=o.ticker,
                    side=o.side,
                    price_dollars=o.price_dollars,
                    count=o.count,
                    filled_count=o.filled_count,
                    status=o.status,
                    created_at=o.created_at.isoformat() if o.created_at else None,
                )
            )
        return GetOrdersResponse(orders=summaries)

    # --- Core: place order with matching ---

    def place_order(self, user_id: str, req: CreateOrderRequestV2) -> CreateOrderResponseV2:
        """Validate, persist, match, update paper state, return V2 response."""
        # 1. Basic validation
        count = _count_to_int(req.count)
        if count <= 0:
            raise ValueError("count must be positive")

        price_cents = _price_to_cents(req.price)
        if not (0 < price_cents < 10000):
            raise ValueError("price out of range")

        # Find market (must exist and be tradeable)
        mkt = self.db.get(Market, req.ticker)
        if not mkt or mkt.status not in ("active", "open"):
            raise ValueError(f"Market {req.ticker} not tradable")

        pa = self._get_or_create_paper_account(user_id)
        pos = self._get_or_create_position(user_id, req.ticker)

        # Rough balance gate for buys (bid = buy yes)
        is_buy = req.side == "bid"
        est_cost_cents = price_cents * count
        if is_buy and pa.balance_cents < est_cost_cents:
            raise ValueError("insufficient paper balance for order")

        # 2. Create DB order record (open)
        order_id = str(uuid4())
        db_order = DBOrder(
            id=order_id,
            user_id=user_id,
            ticker=req.ticker,
            side=req.side,
            action="buy" if is_buy else "sell",
            price_dollars=req.price,
            count=count,
            client_order_id=req.client_order_id,
            time_in_force=req.time_in_force,
            post_only=req.post_only,
            reduce_only=req.reduce_only,
            status="open",
            filled_count=0,
        )
        self.db.add(db_order)
        self.db.commit()
        self.db.refresh(db_order)

        # 3. Matching
        remaining = count
        total_fill_count = 0
        fill_price_sum = Decimal("0")  # for avg
        fill_qty_sum = 0

        # Determine opposite side in book
        ob = get_orderbook(req.ticker)
        opposite_list = ob.no_bids if is_buy else ob.yes_bids  # buy matches asks (no_bids), sell matches bids

        # Sort for matching: for buyer (bid), want lowest ask price first (time then)
        # opposite_list for buy: no_bids (asks), want ascending price
        # for seller: yes_bids, want descending? Standard: match best opp first.
        if is_buy:
            # match cheapest asks first
            opp_sorted = sorted(opposite_list, key=lambda o: (float(o.price_dollars), o.created_ts))
        else:
            # match highest bids first (sellers want high price)
            opp_sorted = sorted(opposite_list, key=lambda o: (-float(o.price_dollars), o.created_ts))

        fills_made = []
        for opp in list(opp_sorted):  # copy because we may mutate
            if remaining <= 0:
                break
            if opp.user_id == user_id:
                # self trade prevention (simple: skip for taker_at_cross)
                if req.self_trade_prevention_type == "taker_at_cross":
                    continue
                # could implement maker cancel etc, skip for MVP

            opp_price_cents = _price_to_cents(opp.price_dollars)
            # Can we cross? For buy: my price >= opp ask price
            # For sell: my price <= opp bid price
            if is_buy and price_cents < opp_price_cents:
                continue
            if not is_buy and price_cents > opp_price_cents:
                continue

            match_qty = min(remaining, opp.remaining_count)
            if match_qty <= 0:
                continue

            # Execute fill at the resting (maker) price
            fill_price_c = opp_price_cents
            fill_price_str = opp.price_dollars

            # Update maker side first (opp order)
            maker_order = self.db.get(DBOrder, opp.order_id)
            if maker_order:
                maker_order.filled_count += match_qty
                if maker_order.filled_count >= maker_order.count:
                    maker_order.status = "filled"
                else:
                    maker_order.status = "partially_filled"
                self.db.add(maker_order)

            # Update taker DB order later
            total_fill_count += match_qty
            fill_price_sum += Decimal(fill_price_str) * match_qty
            fill_qty_sum += match_qty

            # Paper money movement + position
            fill_cents = fill_price_c * match_qty
            maker_user_id = opp.user_id

            # Taker side
            if is_buy:
                # taker buys yes: pay, +yes pos
                pa.balance_cents -= fill_cents
                pos.yes_contracts += match_qty
            else:
                # taker sells yes: receive, -yes pos
                pa.balance_cents += fill_cents
                pos.yes_contracts -= match_qty

            # Maker opposite
            maker_pa = self._get_or_create_paper_account(maker_user_id)
            maker_pos = self._get_or_create_position(maker_user_id, req.ticker)
            if not is_buy:
                # maker was buying yes (bid), now filled as seller to us: pay? Wait:
                # opp was bid (buy yes), we asked (sold yes to them): maker pays us
                maker_pa.balance_cents -= fill_cents
                maker_pos.yes_contracts += match_qty
            else:
                # opp was ask (sell yes), we bid (bought from them): maker receives
                maker_pa.balance_cents += fill_cents
                maker_pos.yes_contracts -= match_qty

            # Record trade for both? Record for taker primarily; maker will see in their fills too if we record symmetrically.
            # For simplicity record one Trade per fill leg for the taker (the incoming)
            trade = Trade(
                id=str(uuid4()),
                user_id=user_id,
                order_id=order_id,
                ticker=req.ticker,
                side=req.side,
                price_dollars=fill_price_str,
                count=match_qty,
            )
            self.db.add(trade)

            # Also record for maker (so their /fills shows it)
            maker_trade = Trade(
                id=str(uuid4()),
                user_id=maker_user_id,
                order_id=opp.order_id,
                ticker=req.ticker,
                side=("ask" if is_buy else "bid"),
                price_dollars=fill_price_str,
                count=match_qty,
            )
            self.db.add(maker_trade)

            # Reduce or remove from book
            opp.remaining_count -= match_qty
            if opp.remaining_count <= 0:
                remove_order(req.ticker, opp.order_id)
            else:
                # keep updated in mem (the dataclass ref)
                pass

            remaining -= match_qty
            fills_made.append(match_qty)

            # commit incrementally? for demo ok at end
            self.db.commit()

        # After matching loop
        avg_fill = None
        if fill_qty_sum > 0:
            avg = (fill_price_sum / Decimal(fill_qty_sum))
            avg_fill = to_dollars_fp(avg, 4)
            db_order.filled_count = total_fill_count
            db_order.avg_fill_price_dollars = avg_fill

        # Handle remaining per TIF
        tif = req.time_in_force
        if remaining > 0:
            if tif == "fill_or_kill" or tif == "immediate_or_cancel":
                # cancel remainder
                db_order.status = "cancelled" if total_fill_count == 0 else "partially_filled"
                # refund any? since no partial debit yet wait: we only debited on fills above
            else:
                # GTC or default: rest the remainder
                db_order.status = "open" if total_fill_count == 0 else "partially_filled"
                # Add to book (convert V2 side)
                side_for_book = req.side
                add_resting_order(
                    ticker=req.ticker,
                    order_id=order_id,
                    user_id=user_id,
                    side=side_for_book,
                    action="buy" if is_buy else "sell",
                    price_dollars=req.price,
                    count=remaining,
                )
                # Note: the DB order already persisted with full original count; book uses remaining
        else:
            db_order.status = "filled"

        self.db.add(db_order)
        self.db.add(pa)
        self.db.add(pos)
        self.db.commit()

        fill_count_fp = to_count_fp(total_fill_count)
        remaining_fp = to_count_fp(remaining)

        return CreateOrderResponseV2(
            order_id=order_id,
            client_order_id=req.client_order_id,
            fill_count=fill_count_fp,
            remaining_count=remaining_fp,
            average_fill_price=avg_fill,
            ts_ms=int(_time.time() * 1000),
        )

    # --- Cancel ---

    def cancel_order(self, user_id: str, order_id: str) -> dict:
        db_order = self.db.get(DBOrder, order_id)
        if not db_order or db_order.user_id != user_id:
            raise ValueError("order not found or not owned by user")
        if db_order.status not in ("open", "partially_filled"):
            return {"cancelled": False, "status": db_order.status}

        db_order.status = "cancelled"
        self.db.add(db_order)

        removed = remove_order(db_order.ticker, order_id)
        self.db.commit()
        return {"cancelled": True, "order_id": order_id, "removed_from_book": removed}

    # --- Admin / settlement ---

    def admin_reset_user(self, username: str) -> dict:
        # Authorization is enforced at the route layer (admin role / club secret).
        user = self.db.query(User).filter(User.username == username).first()
        if not user:
            return {"reset": False, "reason": "no such user"}

        pa = self._get_or_create_paper_account(user.id)
        pa.balance_cents = self.settings.starting_balance_cents

        # cancel all open orders for user
        open_orders = self.db.query(DBOrder).filter(
            DBOrder.user_id == user.id, DBOrder.status.in_(["open", "partially_filled"])
        ).all()
        for o in open_orders:
            o.status = "cancelled"
            remove_order(o.ticker, o.id)

        # zero positions (or leave for history; for reset zero)
        positions = self.db.query(Position).filter(Position.user_id == user.id).all()
        for p in positions:
            p.yes_contracts = 0

        self.db.commit()
        # rebuild books
        rebuild_books_from_db(self.db)
        return {"reset": True, "user": username, "new_balance_cents": pa.balance_cents}

    def admin_force_resolve(self, ticker: str, result: str) -> dict:
        # Authorization is enforced at the route layer (owner role / club secret).
        if result not in ("yes", "no"):
            raise ValueError("result must be yes or no")

        mkt = self.db.get(Market, ticker)
        if not mkt:
            raise ValueError("market not found")

        mkt.status = "settled"
        mkt.result = result
        self.db.add(mkt)

        # settle all positions
        positions = self.db.query(Position).filter(Position.ticker == ticker).all()
        settled_users = 0
        for p in positions:
            if p.yes_contracts == 0:
                continue
            pa = self._get_or_create_paper_account(p.user_id)
            # settlement value: yes win -> +100c per contract, no win -> 0c per (so for long yes: 0)
            payout_per = 100 if result == "yes" else 0
            pnl_cents = p.yes_contracts * payout_per
            pa.balance_cents += pnl_cents
            # record as if a fill? or just adjust balance; zero position
            p.yes_contracts = 0
            settled_users += 1
            self.db.add(pa)
            self.db.add(p)

        self.db.commit()
        # After settlement, cancel any remaining open orders on this market
        open_on_mkt = self.db.query(DBOrder).filter(
            DBOrder.ticker == ticker, DBOrder.status.in_(["open", "partially_filled"])
        ).all()
        for o in open_on_mkt:
            o.status = "cancelled"
            remove_order(ticker, o.id)
        self.db.commit()

        return {
            "resolved": ticker,
            "result": result,
            "users_settled": settled_users,
            "orders_cancelled": len(open_on_mkt),
        }


def get_paper_service(db: Session) -> PaperTradingService:
    """FastAPI dependency factory."""
    return PaperTradingService(db)
