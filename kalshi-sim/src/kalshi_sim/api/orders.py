"""Real Phase 2 order + portfolio endpoints under exact Kalshi /trade-api/v2 paths.

- POST /portfolio/orders (V2 shape: bid/ask, client_order_id, fp strings) -> full matching engine
- GET /portfolio/orders , DELETE /portfolio/orders/{id}
- GET /portfolio/positions , /portfolio/fills , /portfolio/balance
- Also supports /portfolio/events/orders as alias for V2 shape (SDK forward compat)

Uses real RSA verification via require_signed_auth when headers present (or dev bypass).
All side effects hit PaperAccount, Position, Trade, Order tables + in-memory books.
"""

import logging

from fastapi import APIRouter, Depends, Header, HTTPException, Query, Request

from ..db import get_db
from ..models.api import (
    CreateOrderRequestV2,
    CreateOrderResponseV2,
    GetBalanceResponse,
    GetFillsResponse,
    GetOrdersResponse,
    GetPositionsResponse,
)
from ..services.paper_trading import PaperTradingService, get_paper_service
from .auth import ROLE_RANK, get_current_user, require_signed_auth, resolve_admin_rank

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/trade-api/v2", tags=["orders", "portfolio"])


# Also expose the V2 events/orders path that newer docs mention (alias to same handler)
@router.post("/portfolio/events/orders", response_model=CreateOrderResponseV2)
async def create_order_events_v2(
    body: CreateOrderRequestV2,
    db=Depends(get_db),
    access_key: str | None = Header(None, alias="KALSHI-ACCESS-KEY"),
    signature: str | None = Header(None, alias="KALSHI-ACCESS-SIGNATURE"),
    timestamp: str | None = Header(None, alias="KALSHI-ACCESS-TIMESTAMP"),
    x_dev_user: str | None = Header(None, alias="X-Kalshi-Sim-User"),
):
    """V2 shape on the events/orders path (newer docs). Delegates to same engine."""
    return await _handle_create_order(body, db, access_key, signature, timestamp, x_dev_user, path="/trade-api/v2/portfolio/events/orders")


@router.post("/portfolio/orders", response_model=CreateOrderResponseV2)
async def create_order(
    body: CreateOrderRequestV2,
    db=Depends(get_db),
    access_key: str | None = Header(None, alias="KALSHI-ACCESS-KEY"),
    signature: str | None = Header(None, alias="KALSHI-ACCESS-SIGNATURE"),
    timestamp: str | None = Header(None, alias="KALSHI-ACCESS-TIMESTAMP"),
    x_dev_user: str | None = Header(None, alias="X-Kalshi-Sim-User"),
):
    """Primary: POST /trade-api/v2/portfolio/orders using official V2 request shape.
    Real paper matching, balance/position updates, RSA (or bypass) auth.
    SDKs that hit this path will "just work".
    """
    return await _handle_create_order(body, db, access_key, signature, timestamp, x_dev_user, path="/trade-api/v2/portfolio/orders")


async def _handle_create_order(
    body: CreateOrderRequestV2,
    db,
    access_key: str | None,
    signature: str | None,
    timestamp: str | None,
    x_dev_user: str | None,
    path: str,
) -> CreateOrderResponseV2:
    user_id = require_signed_auth(db, "POST", path, access_key, signature, timestamp, x_dev_user)
    svc: PaperTradingService = get_paper_service(db)
    try:
        resp = svc.place_order(user_id, body)
        logger.info(f"Order placed+matched for {user_id} on {body.ticker}: fill={resp.fill_count}")
        return resp
    except ValueError as ve:
        raise HTTPException(400, str(ve))  # noqa: B904
    except Exception:
        logger.exception("place_order failed")
        raise HTTPException(500, "internal error placing paper order")  # noqa: B904


@router.get("/portfolio/orders", response_model=GetOrdersResponse)
async def list_user_orders(
    status: str | None = Query(None),
    db=Depends(get_db),
    user_id: str = Depends(get_current_user),
):
    """User-scoped list of orders (open + history)."""
    svc = get_paper_service(db)
    return svc.get_orders(user_id, status=status)


@router.delete("/portfolio/orders/{order_id}")
async def cancel_user_order(
    order_id: str,
    db=Depends(get_db),
    access_key: str | None = Header(None, alias="KALSHI-ACCESS-KEY"),
    signature: str | None = Header(None, alias="KALSHI-ACCESS-SIGNATURE"),
    timestamp: str | None = Header(None, alias="KALSHI-ACCESS-TIMESTAMP"),
    x_dev_user: str | None = Header(None, alias="X-Kalshi-Sim-User"),
):
    """Cancel a resting paper order. Requires auth."""
    # path for sig
    path = f"/trade-api/v2/portfolio/orders/{order_id}"
    user_id = require_signed_auth(db, "DELETE", path, access_key, signature, timestamp, x_dev_user)
    svc = get_paper_service(db)
    try:
        return svc.cancel_order(user_id, order_id)
    except ValueError as ve:
        raise HTTPException(404, str(ve))  # noqa: B904


# --- Portfolio read endpoints (user scoped, auth required) ---

@router.get("/portfolio/balance", response_model=GetBalanceResponse)
async def get_balance(db=Depends(get_db), user_id: str = Depends(get_current_user)):
    svc = get_paper_service(db)
    return svc.get_balance(user_id)


@router.get("/portfolio/positions", response_model=GetPositionsResponse)
async def get_positions(db=Depends(get_db), user_id: str = Depends(get_current_user)):
    svc = get_paper_service(db)
    return svc.get_positions(user_id)


@router.get("/portfolio/fills", response_model=GetFillsResponse)
async def get_fills(
    limit: int = Query(50, ge=1, le=200),
    db=Depends(get_db),
    user_id: str = Depends(get_current_user),
):
    svc = get_paper_service(db)
    return svc.get_fills(user_id, limit=limit)


# --- Also support common legacy GET /portfolio/orders?status=open style (already covered) ---


# --- Privileged admin actions (for TUI / club ops / teaching). ---
# Authorize with either an admin/owner key (signed KALSHI-ACCESS-* headers) or
# the club admin secret header:  -H "X-Kalshi-Sim-Admin: $CLUB_ADMIN_SECRET"

@router.post("/admin/reset-user")
async def admin_reset(
    request: Request,
    username: str = Query(..., description="username to reset"),
    db=Depends(get_db),
    access_key: str = Header(None, alias="KALSHI-ACCESS-KEY"),
    signature: str = Header(None, alias="KALSHI-ACCESS-SIGNATURE"),
    timestamp: str = Header(None, alias="KALSHI-ACCESS-TIMESTAMP"),
    admin_secret: str = Header(None, alias="X-Kalshi-Sim-Admin"),
):
    """Reset a paper account to starting balance, cancel its orders, zero positions (admin)."""
    rank = resolve_admin_rank(
        db, "POST", request.url.path, access_key, signature, timestamp, admin_secret
    )
    if rank < ROLE_RANK["admin"]:
        raise HTTPException(403, "requires an admin key or the club admin secret")
    svc = get_paper_service(db)
    try:
        return svc.admin_reset_user(username)
    except Exception as e:
        raise HTTPException(400, str(e))  # noqa: B904


@router.post("/admin/resolve/{ticker}")
async def admin_resolve(
    ticker: str,
    request: Request,
    result: str = Query(..., pattern="^(yes|no)$"),
    db=Depends(get_db),
    access_key: str = Header(None, alias="KALSHI-ACCESS-KEY"),
    signature: str = Header(None, alias="KALSHI-ACCESS-SIGNATURE"),
    timestamp: str = Header(None, alias="KALSHI-ACCESS-TIMESTAMP"),
    admin_secret: str = Header(None, alias="X-Kalshi-Sim-Admin"),
):
    """Force-settle a market (owner only — this decides winners). Pays out and cancels orders."""
    rank = resolve_admin_rank(
        db, "POST", request.url.path, access_key, signature, timestamp, admin_secret
    )
    if rank < ROLE_RANK["owner"]:
        raise HTTPException(403, "resolving markets requires the owner role")
    svc = get_paper_service(db)
    try:
        return svc.admin_force_resolve(ticker, result)
    except Exception as e:
        raise HTTPException(400, str(e))  # noqa: B904
