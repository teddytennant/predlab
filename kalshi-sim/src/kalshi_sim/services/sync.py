"""Background / on-demand sync service.

Fetches open markets from real Kalshi public API and upserts into local Market table.
Uses Kalshi ticker as PK. Stores prices as the exact string values returned.
Foundation: called on startup + manual trigger; later APScheduler or Redis worker.
"""
import logging
from datetime import datetime
from typing import Any

from sqlalchemy.orm import Session

from ..clients.kalshi_client import KalshiClient
from ..models.db import Market

logger = logging.getLogger(__name__)


def _parse_market(raw: dict[str, Any]) -> dict[str, Any]:
    """Normalize a Kalshi raw market dict into our DB column names."""
    return {
        "ticker": raw["ticker"],
        "event_ticker": raw.get("event_ticker", ""),
        "title": raw.get("title"),
        "subtitle": raw.get("subtitle"),
        "yes_sub_title": raw.get("yes_sub_title", raw.get("subtitle", "")),
        "no_sub_title": raw.get("no_sub_title", raw.get("subtitle", "")),
        "status": raw.get("status", "active"),
        "market_type": raw.get("market_type", "binary"),
        "open_time": _parse_dt(raw.get("open_time")),
        "close_time": _parse_dt(raw.get("close_time")),
        "latest_expiration_time": _parse_dt(raw.get("latest_expiration_time")),
        "yes_bid_dollars": raw.get("yes_bid_dollars", "0.0000"),
        "yes_ask_dollars": raw.get("yes_ask_dollars", "0.0000"),
        "no_bid_dollars": raw.get("no_bid_dollars", "0.0000"),
        "no_ask_dollars": raw.get("no_ask_dollars", "0.0000"),
        "yes_bid_size_fp": raw.get("yes_bid_size_fp", "0.00"),
        "yes_ask_size_fp": raw.get("yes_ask_size_fp", "0.00"),
        "last_price_dollars": raw.get("last_price_dollars", "0.0000"),
        "volume_fp": raw.get("volume_fp", "0.00"),
        "volume_24h_fp": raw.get("volume_24h_fp", "0.00"),
        "open_interest_fp": raw.get("open_interest_fp", "0.00"),
        "notional_value_dollars": raw.get("notional_value_dollars", "1.0000"),
        "liquidity_dollars": raw.get("liquidity_dollars", "0.0000"),
        "rules_primary": raw.get("rules_primary"),
        "rules_secondary": raw.get("rules_secondary"),
        "raw_json": raw,
    }


def _parse_dt(ts: str | None) -> datetime | None:
    if not ts:
        return None
    # Kalshi returns ISO with Z
    if ts.endswith("Z"):
        ts = ts[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(ts)
    except Exception:
        return None


async def sync_markets(
    db: Session,
    client: KalshiClient,
    limit: int = 50,
    status: str = "open",
) -> int:
    """Fetch up to `limit` markets and upsert them. Returns count upserted."""
    logger.info(f"Syncing markets from Kalshi (status={status}, limit={limit})")

    data = await client.get_markets(limit=limit, status=status)
    markets_raw = data.get("markets", [])

    count = 0
    for raw in markets_raw:
        payload = _parse_market(raw)
        ticker = payload["ticker"]

        existing = db.get(Market, ticker)
        if existing:
            for k, v in payload.items():
                if k != "ticker":
                    setattr(existing, k, v)
            existing.last_synced_at = datetime.utcnow()
        else:
            m = Market(**payload)
            db.add(m)
        count += 1

    db.commit()
    logger.info(f"Synced {count} markets")
    return count


def get_local_markets(db: Session, limit: int = 100, status: str | None = None) -> list[Market]:
    """Query helper used by the API layer."""
    q = db.query(Market)
    if status:
        q = q.filter(Market.status == status)
    return q.order_by(Market.last_synced_at.desc()).limit(limit).all()
