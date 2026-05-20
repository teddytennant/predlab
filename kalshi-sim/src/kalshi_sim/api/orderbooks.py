"""Kalshi orderbook endpoint: GET /trade-api/v2/markets/{ticker}/orderbook

Returns the *exact* production shape: { "orderbook_fp": { "yes_dollars": [[p,s], ...], "no_dollars": [...] } }
"""
from fastapi import APIRouter, Query

from ..services.orderbook import get_orderbook

router = APIRouter(prefix="/trade-api/v2", tags=["orderbook"])


@router.get("/markets/{ticker}/orderbook")
async def get_orderbook_endpoint(ticker: str, depth: int = Query(0, ge=0, le=100)):
    """Exact Kalshi public orderbook response (populated by paper orders + any seeded).
    depth param accepted for compat (not fully enforced in MVP).
    """
    ob = get_orderbook(ticker)
    snap = ob.snapshot()
    return snap
