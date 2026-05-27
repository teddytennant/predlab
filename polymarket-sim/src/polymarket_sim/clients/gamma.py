"""
Thin async httpx client wrapper around the real public Polymarket Gamma API.

Used exclusively by the sync service to fetch live market data.
Only public unauthenticated endpoints — never writes.
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any

import httpx

from ..config import settings

logger = logging.getLogger(__name__)


def _volume_of(market: dict[str, Any]) -> float:
    try:
        return float(market.get("volume") or 0.0)
    except (TypeError, ValueError):
        return 0.0


class GammaClient:
    """Client for https://gamma-api.polymarket.com"""

    def __init__(self, base_url: str | None = None, timeout: float = 20.0) -> None:
        self.base_url = base_url or settings.gamma_api_base
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=timeout,
            headers={"User-Agent": "polymarket-sim/0.1 (educational)"},
        )

    async def _get_page(self, offset: int, page_size: int) -> list[dict[str, Any]]:
        """One page of active, non-closed markets, volume-descending.

        Returns ``[]`` on any error or non-list response. Gamma 422s once you
        page past its ~10k offset cap, so treating errors as end-of-data lets
        the caller stop cleanly.
        """
        url = (
            f"/markets?limit={page_size}&offset={offset}"
            "&active=true&closed=false&order=volumeNum&ascending=false"
        )
        try:
            resp = await self._client.get(url)
            resp.raise_for_status()
            data = resp.json()
        except Exception as exc:
            logger.warning("Gamma page fetch stopped at offset %d: %s", offset, exc)
            return []
        if not isinstance(data, list):
            logger.warning("Unexpected Gamma response shape at offset %d: %s", offset, type(data))
            return []
        return data

    async def fetch_markets_by_volume(
        self,
        *,
        min_volume: float,
        max_markets: int,
        page_size: int = 100,
        pace_seconds: float = 0.25,
    ) -> list[dict[str, Any]]:
        """Page through Gamma volume-descending, collecting active, non-closed
        markets at or above ``min_volume`` up to ``max_markets``.

        Walks pages until it hits the volume floor, Gamma's offset cap, an
        empty/short page, or ``max_markets`` — whichever comes first. Because
        the feed is sorted by volume descending, the first market below the
        floor means everything after it is too, so we stop immediately.
        """
        collected: list[dict[str, Any]] = []
        offset = 0
        while len(collected) < max_markets:
            page = await self._get_page(offset, page_size)
            if not page:
                break
            for market in page:
                if _volume_of(market) < min_volume:
                    return collected
                collected.append(market)
                if len(collected) >= max_markets:
                    return collected
            if len(page) < page_size:
                break  # last page Gamma had
            offset += len(page)
            if pace_seconds:
                await asyncio.sleep(pace_seconds)
        return collected

    async def close(self) -> None:
        await self._client.aclose()
