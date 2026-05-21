"""
Stub data sync service.

Periodically (or on-demand) fetches live markets from the real Gamma public API
and upserts them into our local DB so that /markets returns real, up-to-date data
(approximately 20 active markets with current outcomePrices, bestBid/Ask, etc.).

Phase 1: simple one-shot sync at startup + manual trigger. Background loop added later.
"""

from __future__ import annotations

import json
import logging
from datetime import datetime
from typing import Any

from sqlalchemy import select
from sqlalchemy.orm import Session

from ..clients.gamma import GammaClient
from ..models.db import Market

logger = logging.getLogger(__name__)


def _parse_price_list(raw: str | list[str] | None) -> list[str]:
    if raw is None:
        return ["0.5", "0.5"]
    if isinstance(raw, list):
        return [str(x) for x in raw]
    try:
        parsed = json.loads(raw)
        return [str(x) for x in parsed] if isinstance(parsed, list) else ["0.5", "0.5"]
    except Exception:
        return ["0.5", "0.5"]


def _parse_outcomes(raw: str | list[str] | None) -> list[str]:
    if raw is None:
        return ["Yes", "No"]
    if isinstance(raw, list):
        return [str(x) for x in raw]
    try:
        parsed = json.loads(raw)
        return [str(x) for x in parsed] if isinstance(parsed, list) else ["Yes", "No"]
    except Exception:
        return ["Yes", "No"]


def _safe_float(val: Any) -> float | None:
    if val is None:
        return None
    try:
        return float(val)
    except (TypeError, ValueError):
        return None


async def sync_markets_from_gamma(db: Session, limit: int = 200) -> int:
    """Fetch from Gamma and upsert into local Market table. Returns count upserted."""
    client = GammaClient()
    try:
        raw_markets = await client.fetch_active_markets(limit=limit)
    finally:
        await client.close()

    if not raw_markets:
        logger.warning("No markets returned from Gamma — sync skipped.")
        return 0

    upserted = 0
    for raw in raw_markets:
        market_id = str(raw.get("id"))
        if not market_id:
            continue

        condition_id = str(raw.get("conditionId") or raw.get("condition_id") or "")
        question = str(raw.get("question", "Unknown market"))[:500]
        slug = str(raw.get("slug", market_id))[:120]

        outcomes = _parse_outcomes(raw.get("outcomes"))
        outcome_prices = _parse_price_list(raw.get("outcomePrices"))

        clob_ids = raw.get("clobTokenIds")
        if isinstance(clob_ids, str):
            try:
                clob_ids = json.loads(clob_ids)
            except Exception:
                clob_ids = None
        if not isinstance(clob_ids, list):
            clob_ids = None

        best_bid = _safe_float(raw.get("bestBid"))
        best_ask = _safe_float(raw.get("bestAsk"))
        last_trade = _safe_float(raw.get("lastTradePrice"))
        volume = _safe_float(raw.get("volume"))
        liquidity = _safe_float(raw.get("liquidity"))

        # Compute spread if both sides present
        spread = None
        if best_bid is not None and best_ask is not None:
            spread = round(best_ask - best_bid, 6)

        # Dates
        def parse_date(s: Any) -> datetime | None:
            if not s:
                return None
            try:
                # Strip Z and parse
                s = s.replace("Z", "+00:00")
                return datetime.fromisoformat(s.replace("Z", "")).replace(tzinfo=None)
            except Exception:
                return None

        start = parse_date(raw.get("startDate"))
        end = parse_date(raw.get("endDate"))

        # Upsert
        existing = db.execute(select(Market).where(Market.id == market_id)).scalar_one_or_none()

        now = datetime.utcnow()
        if existing:
            existing.condition_id = condition_id or existing.condition_id
            existing.question = question
            existing.slug = slug
            existing.outcomes = outcomes
            existing.outcome_prices = outcome_prices
            existing.clob_token_ids = clob_ids
            existing.best_bid = best_bid
            existing.best_ask = best_ask
            existing.last_trade_price = last_trade
            existing.spread = spread
            existing.volume = volume
            existing.liquidity = liquidity
            existing.active = bool(raw.get("active", True))
            existing.closed = bool(raw.get("closed", False))
            existing.start_date = start or existing.start_date
            existing.end_date = end or existing.end_date
            existing.last_synced_at = now
            existing.updated_at = now
        else:
            m = Market(
                id=market_id,
                condition_id=condition_id,
                question=question,
                slug=slug,
                outcomes=outcomes,
                outcome_prices=outcome_prices,
                clob_token_ids=clob_ids,
                best_bid=best_bid,
                best_ask=best_ask,
                last_trade_price=last_trade,
                spread=spread,
                volume=volume,
                liquidity=liquidity,
                active=bool(raw.get("active", True)),
                closed=bool(raw.get("closed", False)),
                start_date=start,
                end_date=end,
                last_synced_at=now,
            )
            db.add(m)
        upserted += 1

    db.commit()
    logger.info("Gamma sync complete: upserted/updated %d markets", upserted)
    return upserted
