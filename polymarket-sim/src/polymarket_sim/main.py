"""
FastAPI application factory for the Polymarket API Simulator (Phase 2 Fidelity).

- Full P0/P1 CLOB surface for SDK compatibility: /book, /midpoint, /spread, /last-trade-price,
  POST /order + /orders, DELETE /order, user data paths (/data/orders, /data/trades)
- Paper trading engine: auth via paper API keys (POLY_API_KEY header), balance escrow,
  position updates, mark-to-market P&L, manual + auto settlement hooks
- GET /markets (enhanced filters + pagination stub), /events (gamma pass-through)
- All responses shaped for drop-in use by py-clob-client-v2 and raw httpx
- Admin ops gated by X-Admin-Secret (reset, force resolve)

See PHASE2.md for verification commands and paper money model.

Run with:
    uvicorn polymarket_sim.main:app --reload --port 8001
"""

from __future__ import annotations

import logging
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from datetime import datetime
from typing import Any

from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy import select
from sqlalchemy.orm import Session

from .config import get_settings
from .db import get_session, init_db
from .models.db import Market, Order, User
from .models.schemas import (
    HealthResponse,
    LastTradePriceOut,
    MarketOut,
    MidpointOut,
    OrderCreate,
    OrderOut,
    PositionWithPnLOut,
    PostOrderResponse,
    SpreadOut,
    UserOrderOut,
)
from .services.auth import (
    ROLE_RANK,
    VALID_ROLES,
    Principal,
    create_demo_user_with_key,
    get_current_user,
    require_role,
)
from .services.orderbook import OrderBookEntry, get_orderbook
from .services.paper_trading import (
    cancel_paper_order,
    delete_user,
    force_resolve_market,
    leaderboard,
    list_user_open_orders,
    list_user_positions_with_pnl,
    place_paper_order,
    reset_all_to_starting,
    reset_user_to_starting,
)
from .services.sync import sync_markets_from_gamma

# Logging
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s | %(levelname)s | %(name)s | %(message)s"
)
logger = logging.getLogger("polymarket_sim")


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Startup / shutdown lifecycle (Phase 2)."""
    settings = get_settings()
    logger.info("Starting polymarket-sim in %s mode", settings.environment)

    # 1. Ensure DB tables exist
    init_db()

    # 2. One-time initial sync of live markets
    from .db import SessionLocal

    db = SessionLocal()
    try:
        count = await sync_markets_from_gamma(db, limit=30)
        logger.info("Initial market sync complete: %d markets loaded", count)

        # 3. Hydrate in-memory orderbooks from any open DB orders (survive restart)
        _hydrate_orderbooks_from_db(db)
        logger.info("Orderbooks hydrated from persisted open orders")

        # 4. Optional: in dev, ensure at least one demo paper user exists
        if settings.is_dev:
            _ensure_dev_demo_user(db)
    except Exception as exc:
        logger.exception("Startup work (sync/hydrate) failed (non-fatal): %s", exc)
    finally:
        db.close()

    yield
    logger.info("Shutting down polymarket-sim")


def _hydrate_orderbooks_from_db(db: Session) -> None:
    """Load open/partial paper orders into the global in-memory books (best-effort for restart)."""
    open_orders = (
        db.execute(select(Order).where(Order.status.in_(["open", "partial"]))).scalars().all()
    )
    for o in open_orders:
        if not o.clob_token_id:
            continue
        book = get_orderbook(o.clob_token_id)
        entry = OrderBookEntry(
            id=o.id,
            user_id=o.user_id,
            price=o.price or 0.5,
            size=max(0.0, o.size - o.filled_size),
            side=o.side,
        )
        if o.side == "buy":
            book.bids.append(entry)
        else:
            book.asks.append(entry)
    # Re-sort any books that received entries (access private registry safely)
    try:
        from .services import orderbook as _ob_mod

        _order_books = getattr(_ob_mod, "_order_books", {})
        for b in _order_books.values():
            b._sort()
    except Exception:
        pass  # non-fatal; books will work for new orders


def _ensure_dev_demo_user(db: Session) -> None:
    """Create a default demo paper user if none exists (dev convenience)."""
    from .models.db import User

    demo = db.execute(select(User).where(User.username == "demo_trader")).scalar_one_or_none()
    if demo:
        return
    try:
        user, key, secret = create_demo_user_with_key(db, "demo_trader", "Demo Trader (Phase 2)")
        logger.warning(
            "DEV DEMO USER CREATED — username=demo_trader paper_key=%s secret=%s (store secret securely)",
            key,
            secret,
        )
    except Exception as e:
        logger.info("Demo user already present or creation skipped: %s", e)


def create_app() -> FastAPI:
    """Application factory (Phase 2 Fidelity)."""
    settings = get_settings()

    app = FastAPI(
        title="Polymarket API Simulator",
        description=(
            "Educational paper-trading clone of Polymarket's public + CLOB APIs. "
            "Live market data synced from Gamma. All trading uses paper money only. "
            "100% shape-compatible for existing SDKs / py-clob-client. "
            "NOT AFFILIATED WITH POLYMARKET."
        ),
        version="0.2.0-phase2",
        lifespan=lifespan,
    )

    # Allow the terminal website (and any localhost dev) to call the API from browser
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],  # dev only — tighten for production
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # ------------------------------------------------------------------
    # System
    # ------------------------------------------------------------------
    @app.get("/health", response_model=HealthResponse, tags=["system"])
    async def health() -> HealthResponse:
        return HealthResponse(status="ok", environment=settings.environment)

    # ------------------------------------------------------------------
    # Public market data (Gamma fidelity + filters + pagination stub)
    # ------------------------------------------------------------------
    @app.get("/markets", response_model=list[MarketOut], tags=["markets"])
    async def list_markets(
        active: bool = True,
        limit: int = 50,
        offset: int = 0,
        q: str | None = None,  # simple search stub on question
        session: Session = Depends(get_session),
    ) -> list[MarketOut]:
        """Full GET /markets with filters and pagination stub.

        Returns real live data from Gamma sync. Shape matches real Gamma for SDKs.
        """
        stmt = select(Market)
        if active:
            stmt = stmt.where(Market.active, ~Market.closed)
        if q:
            stmt = stmt.where(Market.question.ilike(f"%{q}%"))
        stmt = stmt.order_by(Market.volume.desc().nullslast()).offset(offset).limit(limit)
        markets = session.execute(stmt).scalars().all()

        out: list[MarketOut] = []
        for m in markets:
            out.append(
                MarketOut(
                    id=m.id,
                    conditionId=m.condition_id,
                    question=m.question,
                    slug=m.slug,
                    outcomes=m.outcomes or ["Yes", "No"],
                    outcomePrices=m.outcome_prices or ["0.5", "0.5"],
                    clobTokenIds=m.clob_token_ids,
                    bestBid=float(m.best_bid) if m.best_bid is not None else None,
                    bestAsk=float(m.best_ask) if m.best_ask is not None else None,
                    lastTradePrice=float(m.last_trade_price)
                    if m.last_trade_price is not None
                    else None,
                    volume=float(m.volume) if m.volume is not None else None,
                    liquidity=float(m.liquidity) if m.liquidity is not None else None,
                    active=m.active,
                    closed=m.closed,
                    updatedAt=m.updated_at,
                )
            )
        return out

    @app.get("/events", tags=["markets"])
    async def list_events(limit: int = 10) -> list[dict[str, Any]]:
        """GET /events — pass-through to real Gamma for fidelity (P0)."""
        from .clients.gamma import GammaClient

        client = GammaClient()
        try:
            raw = await client.fetch_active_markets(limit=limit)  # reuse; real /events similar
            # For true events one would add fetch_events to client; stub returns markets for now
            return raw[:limit]
        except Exception as exc:
            logger.warning("Events fetch failed: %s", exc)
            return []
        finally:
            await client.close()

    # ------------------------------------------------------------------
    # CLOB public helpers (exact paths used by py-clob-client-v2)
    # ------------------------------------------------------------------
    def _book_snapshot(token_id: str) -> dict[str, Any]:
        """Return CLOB-shaped book. Uses live paper depth + falls back to market data for display."""
        book = get_orderbook(token_id)
        snap = book.snapshot()

        bids = [{"price": str(e["price"]), "size": str(e["size"])} for e in snap.get("bids", [])]
        asks = [{"price": str(e["price"]), "size": str(e["size"])} for e in snap.get("asks", [])]

        # Real depth is always from paper orders only. Synthetic display can be added later from market.
        return {
            "bids": bids,
            "asks": asks,
            "asset_id": token_id,
            "timestamp": datetime.utcnow().isoformat() + "Z",
        }

    @app.get("/book", tags=["clob"])
    async def get_book(token_id: str, session: Session = Depends(get_session)) -> dict[str, Any]:
        """GET /book?token_id=... — real Polymarket shape for SDKs."""
        data = _book_snapshot(token_id)
        return data

    @app.post("/books", tags=["clob"])
    async def get_books(payload: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Batch books (used by client)."""
        results = []
        for p in payload:
            tid = p.get("token_id") or p.get("asset_id")
            if tid:
                results.append(_book_snapshot(str(tid)))
        return results

    @app.get("/midpoint", response_model=MidpointOut, tags=["clob"])
    async def get_midpoint(token_id: str, session: Session = Depends(get_session)) -> MidpointOut:
        m = _find_market_for_token(session, token_id)
        mid = "0.5"
        if m and m.best_bid is not None and m.best_ask is not None:
            mid = str(round((float(m.best_bid) + float(m.best_ask)) / 2, 6))
        elif m and m.last_trade_price is not None:
            mid = str(m.last_trade_price)
        return MidpointOut(midpoint=mid)

    @app.get("/spread", response_model=SpreadOut, tags=["clob"])
    async def get_spread(token_id: str, session: Session = Depends(get_session)) -> SpreadOut:
        m = _find_market_for_token(session, token_id)
        spr = "0"
        if m and m.best_bid is not None and m.best_ask is not None:
            spr = str(round(float(m.best_ask) - float(m.best_bid), 6))
        return SpreadOut(spread=spr)

    @app.get("/last-trade-price", response_model=LastTradePriceOut, tags=["clob"])
    async def get_last_trade_price(
        token_id: str, session: Session = Depends(get_session)
    ) -> LastTradePriceOut:
        m = _find_market_for_token(session, token_id)
        val = "0.5"
        if m and m.last_trade_price is not None:
            val = str(m.last_trade_price)
        return LastTradePriceOut(lastTradePrice=val)

    # ------------------------------------------------------------------
    # Authenticated trading (paper money) — real POST /order + DELETE
    # ------------------------------------------------------------------
    async def _parse_order_payload(request: Request) -> dict[str, Any]:
        """Accept both simplified and real signed order payloads from clients."""
        try:
            body = await request.json()
        except Exception:
            body = {}
        # Normalize common fields from real clob payload or our simple
        token_id = (
            body.get("tokenId")
            or body.get("token_id")
            or body.get("asset_id")
            or body.get("clob_token_id")
        )
        side_raw = (body.get("side") or "").upper()
        side = (
            "buy"
            if side_raw in ("BUY", "buy")
            else "sell"
            if side_raw in ("SELL", "sell")
            else "buy"
        )
        price = body.get("price")
        if price is not None:
            try:
                price = float(price)
            except Exception:
                price = None
        size = body.get("size") or body.get("amount") or 0
        try:
            size = float(size)
        except Exception:
            size = 0.1
        market_id = body.get("market") or body.get("market_id")
        return {
            "token_id": str(token_id) if token_id else None,
            "side": side,
            "price": price,
            "size": size,
            "market_id": str(market_id) if market_id else None,
            "raw": body,
        }

    @app.post("/order", response_model=PostOrderResponse, status_code=200, tags=["trading"])
    async def post_single_order(
        request: Request,
        user: User = Depends(get_current_user),
        session: Session = Depends(get_session),
    ) -> PostOrderResponse:
        """POST /order — primary path used by py-clob-client create_and_post_order."""
        parsed = await _parse_order_payload(request)
        if not parsed.get("token_id") or parsed["size"] <= 0:
            return PostOrderResponse(
                success=False, orderID="0", status="error", errorMsg="bad payload"
            )

        # Resolve market if needed via token
        market_id = parsed.get("market_id")
        if not market_id:
            m = _find_market_for_token(session, parsed["token_id"])
            if m:
                market_id = m.id
            else:
                return PostOrderResponse(
                    success=False, orderID="0", status="error", errorMsg="unknown token"
                )

        try:
            order = place_paper_order(
                session,
                user,
                market_id=market_id,
                clob_token_id=parsed["token_id"],
                side=parsed["side"],
                price=parsed["price"],
                size=parsed["size"],
                order_type="limit" if parsed["price"] is not None else "market",
            )
            return PostOrderResponse(
                success=True,
                orderID=str(order.id),
                status=order.status,
            )
        except Exception as exc:
            logger.exception("place_paper_order failed: %s", exc)
            return PostOrderResponse(
                success=False, orderID="0", status="error", errorMsg=str(exc)[:200]
            )

    @app.post("/orders", response_model=list[PostOrderResponse], status_code=200, tags=["trading"])
    async def post_batch_orders(
        request: Request,
        user: User = Depends(get_current_user),
        session: Session = Depends(get_session),
    ) -> list[PostOrderResponse]:
        """POST /orders batch (client support)."""
        try:
            body = await request.json()
            if not isinstance(body, list):
                body = [body]
        except Exception:
            body = []
        results = []
        for item in body[:10]:  # safety
            # Reuse single logic by faking request? Simpler: call place directly
            # For brevity in this slice we treat batch as multiple singles
            # (real client may send array of signed)
            parsed = {
                "token_id": item.get("tokenId") or item.get("asset_id"),
                "side": "buy" if str(item.get("side", "")).upper() == "BUY" else "sell",
                "price": float(item.get("price")) if item.get("price") is not None else None,
                "size": float(item.get("size", 0)),
                "market_id": item.get("market"),
            }
            try:
                m = _find_market_for_token(session, parsed.get("token_id") or "")
                mkt = parsed["market_id"] or (m.id if m else None)
                if not mkt or not parsed.get("token_id"):
                    results.append(PostOrderResponse(success=False, orderID="0", status="error"))
                    continue
                    results.append(PostOrderResponse(success=False, orderID="0", status="error"))
                    continue
                o = place_paper_order(
                    session,
                    user,
                    mkt,
                    parsed["token_id"],
                    parsed["side"],
                    parsed["price"],
                    parsed["size"],
                )
                results.append(PostOrderResponse(success=True, orderID=str(o.id), status=o.status))
            except Exception:
                results.append(PostOrderResponse(success=False, orderID="0", status="error"))
        return results

    def _find_market_for_token(session: Session, token_id: str) -> Market | None:
        markets = session.execute(select(Market)).scalars().all()
        for m in markets:
            if m.clob_token_ids and token_id in [str(t) for t in m.clob_token_ids]:
                return m
        return None

    @app.delete("/order", tags=["trading"])
    async def cancel_single(
        request: Request,
        user: User = Depends(get_current_user),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """DELETE /order (body: {"orderID": "123"} or similar)."""
        try:
            body = await request.json()
        except Exception:
            body = {}
        oid = body.get("orderID") or body.get("orderId") or body.get("id")
        if not oid:
            raise HTTPException(400, "orderID required in body")
        try:
            order = cancel_paper_order(session, user, int(oid))
            return {"success": True, "orderID": str(order.id), "status": order.status}
        except Exception as exc:
            raise HTTPException(400, str(exc)) from None

    # ------------------------------------------------------------------
    # User-scoped paper data (P1 requirement)
    # ------------------------------------------------------------------
    @app.get("/positions", response_model=list[PositionWithPnLOut], tags=["portfolio"])
    async def get_positions(
        user: User = Depends(get_current_user),
        session: Session = Depends(get_session),
    ) -> list[PositionWithPnLOut]:
        """User positions + live mark-to-market P&L using synced prices."""
        raw = list_user_positions_with_pnl(session, user)
        return [PositionWithPnLOut(**p) for p in raw]

    @app.get("/user/orders", response_model=list[UserOrderOut], tags=["portfolio"])
    async def get_user_orders(
        user: User = Depends(get_current_user),
        session: Session = Depends(get_session),
    ) -> list[UserOrderOut]:
        """Basic /user/orders (open)."""
        orders = list_user_open_orders(session, user)
        return [
            UserOrderOut(
                id=o.id,
                market_id=o.market_id,
                clob_token_id=o.clob_token_id,
                side=o.side,
                price=o.price,
                size=o.size,
                filled_size=o.filled_size,
                status=o.status,
                created_at=o.created_at,
            )
            for o in orders
        ]

    # Data paths for clob-client compatibility (/data/orders etc)
    @app.get("/data/orders", tags=["portfolio"])
    async def data_orders(
        user: User = Depends(get_current_user), session: Session = Depends(get_session)
    ) -> dict[str, Any]:
        orders = list_user_open_orders(session, user)
        return {
            "data": [
                {
                    "id": str(o.id),
                    "market": o.market_id,
                    "asset_id": o.clob_token_id,
                    "side": o.side,
                    "price": o.price,
                    "size": o.size,
                    "status": o.status,
                }
                for o in orders
            ],
            "next_cursor": "END",
        }

    @app.get("/data/trades", tags=["portfolio"])
    async def data_trades(
        user: User = Depends(get_current_user), session: Session = Depends(get_session)
    ) -> dict[str, Any]:
        # Minimal: return recent trades for the user
        from .models.db import Trade

        trades = (
            session.execute(
                select(Trade)
                .where(Trade.user_id == user.id)
                .order_by(Trade.created_at.desc())
                .limit(50)
            )
            .scalars()
            .all()
        )
        return {
            "data": [
                {
                    "id": str(t.id),
                    "market": t.market_id,
                    "asset_id": t.clob_token_id,
                    "side": t.side,
                    "price": t.price,
                    "size": t.size,
                    "created_at": t.created_at.isoformat(),
                }
                for t in trades
            ],
            "next_cursor": "END",
        }

    # ------------------------------------------------------------------
    # Admin privileged endpoints (X-Admin-Secret header)
    # ------------------------------------------------------------------
    @app.post("/admin/create-paper-key", tags=["admin"])
    async def admin_create_key(
        username: str,
        display_name: str | None = None,
        role: str = "member",
        principal: Principal = Depends(require_role("admin")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Create a new paper user + return the key (secret shown once).

        Any admin can mint a member key; only an owner can mint admin/owner keys.
        """
        role = role.lower()
        if role not in VALID_ROLES:
            raise HTTPException(400, f"invalid role (one of {sorted(VALID_ROLES)})")
        if ROLE_RANK[role] >= ROLE_RANK["admin"] and principal.rank < ROLE_RANK["owner"]:
            raise HTTPException(403, "only an owner can grant admin/owner roles")
        try:
            user, key, secret = create_demo_user_with_key(session, username, display_name, role)
            return {
                "username": user.username,
                "role": user.role,
                "api_key": key,
                "secret": secret,
                "note": "Store the secret securely. Use api_key in POLY_API_KEY header.",
            }
        except ValueError as ve:
            raise HTTPException(400, str(ve)) from None

    @app.post("/admin/set-role", tags=["admin"])
    async def admin_set_role(
        username: str,
        role: str,
        _: Principal = Depends(require_role("owner")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Promote/demote an existing user (owner only)."""
        role = role.lower()
        if role not in VALID_ROLES:
            raise HTTPException(400, f"invalid role (one of {sorted(VALID_ROLES)})")
        u = session.execute(select(User).where(User.username == username)).scalar_one_or_none()
        if not u:
            raise HTTPException(404, "user not found")
        u.role = role
        session.commit()
        return {"username": username, "role": role}

    @app.post("/admin/revoke-key", tags=["admin"])
    async def admin_revoke_key(
        username: str,
        _: Principal = Depends(require_role("admin")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Deactivate all of a user's API keys (admin)."""
        from .models.db import ApiKey

        u = session.execute(select(User).where(User.username == username)).scalar_one_or_none()
        if not u:
            raise HTTPException(404, "user not found")
        keys = session.execute(select(ApiKey).where(ApiKey.user_id == u.id)).scalars().all()
        for k in keys:
            k.is_active = False
        session.commit()
        return {"revoked": username, "keys_disabled": len(keys)}

    @app.post("/admin/reset-balance", tags=["admin"])
    async def admin_reset_balance(
        username: str | None = None,
        _: Principal = Depends(require_role("admin")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Reset one or all members to a clean starting state (teaching resets).

        Cancels open orders and clears positions as well as restoring cash, so a
        reset member's net worth is exactly the starting balance. Omit
        ``username`` to wipe everyone (e.g. before a new competition).
        """
        if username:
            u = session.execute(select(User).where(User.username == username)).scalar_one_or_none()
            if not u:
                raise HTTPException(404, "user not found")
            return reset_user_to_starting(session, u, settings.starting_balance_usd)
        return reset_all_to_starting(session, settings.starting_balance_usd)

    @app.post("/admin/delete-user", tags=["admin"])
    async def admin_delete_user(
        username: str,
        _: Principal = Depends(require_role("admin")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Permanently remove a member and all their data (they left the club)."""
        try:
            return delete_user(session, username)
        except ValueError as ve:
            raise HTTPException(404, str(ve)) from None

    @app.post("/admin/force-resolve", tags=["admin"])
    async def admin_force_resolve(
        market_id: str,
        resolution: str = "yes",
        _: Principal = Depends(require_role("owner")),
        session: Session = Depends(get_session),
    ) -> dict[str, Any]:
        """Force settlement for a market (owner only — this decides winners)."""
        return force_resolve_market(session, market_id, resolution)

    @app.get("/admin/leaderboard", tags=["admin"])
    async def admin_leaderboard(
        _: Principal = Depends(require_role("admin")),
        session: Session = Depends(get_session),
    ) -> list[dict[str, Any]]:
        """Club standings: every user ranked by paper net worth (admin only)."""
        return leaderboard(session)

    # Legacy stub kept for Phase 1 scripts
    @app.post("/orders", response_model=OrderOut, status_code=201, tags=["trading", "legacy"])
    async def legacy_create_order(
        body: OrderCreate,
        session: Session = Depends(get_session),
    ) -> OrderOut:
        """Legacy Phase-1 style (no auth). For quick tests only."""
        # Use demo user if present, else first user
        demo = session.execute(
            select(User).where(User.username == "demo_trader")
        ).scalar_one_or_none()
        user_id = demo.id if demo else 1
        market = session.get(Market, body.market_id)
        if not market:
            raise HTTPException(404, "Market not found")
        token = body.clob_token_id or (market.clob_token_ids[0] if market.clob_token_ids else "0")
        effective_user: User = demo or session.get(User, user_id) or session.get(User, 1)  # type: ignore[assignment]
        order = place_paper_order(
            session,
            effective_user,
            body.market_id,
            token,
            body.side,
            body.price,
            body.size,
        )
        return OrderOut(
            id=order.id,
            market_id=order.market_id,
            side=order.side,
            price=order.price,
            size=order.size,
            filled_size=order.filled_size,
            status=order.status,
            created_at=order.created_at,
        )

    return app


# For `uvicorn polymarket_sim.main:app` and `python -m` entry
app = create_app()


if __name__ == "__main__":
    import uvicorn

    settings = get_settings()
    logger.info("Running directly via __main__ on %s:%d", settings.host, settings.port)
    uvicorn.run(
        "polymarket_sim.main:app",
        host=settings.host,
        port=settings.port,
        reload=settings.is_dev,
    )
