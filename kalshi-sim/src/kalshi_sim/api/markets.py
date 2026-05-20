"""Kalshi-shaped market data endpoints under /trade-api/v2/markets*"""
from __future__ import annotations

import logging
from typing import Optional

from fastapi import APIRouter, Depends, Query
from sqlalchemy.orm import Session

from ..clients.kalshi_client import KalshiClient, get_kalshi_client
from ..db import get_db
from ..models.api import GetMarketsResponse, MarketResponse
from ..models.db import Market as DBMarket
from ..services.sync import get_local_markets, sync_markets

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/trade-api/v2", tags=["markets"])


@router.get("/markets", response_model=GetMarketsResponse)
async def list_markets(
    limit: int = Query(20, ge=1, le=200),
    status: str = Query("open", description="open|active|closed|settled|..."),
    cursor: Optional[str] = None,
    event_ticker: Optional[str] = Query(None, description="filter by event"),
    series_ticker: Optional[str] = None,
    db=Depends(get_db),
    client: KalshiClient = Depends(get_kalshi_client),
):
    """Kalshi-compatible GET /trade-api/v2/markets with richer filters (event_ticker etc).

    Live synced + simulated=True marker.
    """
    sync_status = status
    db_status = "active" if status in ("open", "active") else status
    local = get_local_markets(db, limit=limit, status=db_status if db_status != "all" else None)

    # Apply extra filters in python for Phase 2 (could push to SQL)
    if event_ticker:
        local = [m for m in local if m.event_ticker == event_ticker]
    if series_ticker:
        local = [m for m in local if getattr(m, "series_ticker", "") == series_ticker][:limit]

    if not local:
        logger.info("No local markets — performing initial sync")
        await sync_markets(db, client, limit=limit, status=sync_status)
        local = get_local_markets(db, limit=limit, status=db_status if db_status != "all" else None)
        if event_ticker:
            local = [m for m in local if m.event_ticker == event_ticker]

    markets = []
    for m in local[:limit]:
        markets.append(
            MarketResponse(
                ticker=m.ticker,
                event_ticker=m.event_ticker,
                market_type=m.market_type,
                title=m.title,
                subtitle=m.subtitle,
                yes_sub_title=m.yes_sub_title,
                no_sub_title=m.no_sub_title,
                status=m.status,
                yes_bid_dollars=m.yes_bid_dollars,
                yes_ask_dollars=m.yes_ask_dollars,
                no_bid_dollars=m.no_bid_dollars,
                no_ask_dollars=m.no_ask_dollars,
                last_price_dollars=m.last_price_dollars,
                volume_fp=m.volume_fp,
                volume_24h_fp=m.volume_24h_fp,
                open_interest_fp=m.open_interest_fp,
                notional_value_dollars=m.notional_value_dollars,
                rules_primary=m.rules_primary,
                rules_secondary=m.rules_secondary,
                simulated=True,
            )
        )

    return GetMarketsResponse(markets=markets, cursor="")


@router.get("/events")
async def list_events(
    limit: int = Query(20, ge=1, le=200),
    status: str = Query("open"),
    db=Depends(get_db),
):
    """Basic /trade-api/v2/events stub.
    Returns unique events derived from synced markets (grouped by event_ticker).
    Sufficient for SDK discovery + club use until full event sync.
    """
    markets = get_local_markets(db, limit=200, status=status if status != "all" else None)
    events_map: dict[str, dict] = {}
    for m in markets:
        et = m.event_ticker or m.ticker.split("-")[0]
        if et not in events_map:
            events_map[et] = {
                "event_ticker": et,
                "title": m.title or et,
                "status": m.status,
                "markets": [],
            }
        events_map[et]["markets"].append(m.ticker)
    events = list(events_map.values())[:limit]
    return {"events": events, "cursor": ""}


@router.get("/markets/{ticker}", response_model=MarketResponse)
async def get_market(ticker: str, db: "Session" = Depends(get_db)) -> MarketResponse:
    m = db.get(DBMarket, ticker)
    if not m:
        # fallback: return 404 or empty stub (for now let it 404 cleanly)
        from fastapi import HTTPException
        raise HTTPException(404, f"Market {ticker} not found (try /markets first to sync)")
    return MarketResponse(
        ticker=m.ticker,
        event_ticker=m.event_ticker,
        yes_sub_title=m.yes_sub_title,
        no_sub_title=m.no_sub_title,
        status=m.status,
        yes_bid_dollars=m.yes_bid_dollars,
        yes_ask_dollars=m.yes_ask_dollars,
        no_bid_dollars=m.no_bid_dollars,
        no_ask_dollars=m.no_ask_dollars,
        last_price_dollars=m.last_price_dollars,
        volume_fp=m.volume_fp,
        simulated=True,
    )
