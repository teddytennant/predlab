"""
Thin async httpx client wrapper around the real public Polymarket Gamma API.

Used exclusively by the sync service to fetch live market data.
Only public unauthenticated endpoints — never writes.
"""

from __future__ import annotations

import logging
from typing import Any

import httpx

from ..config import settings

logger = logging.getLogger(__name__)


class GammaClient:
    """Client for https://gamma-api.polymarket.com"""

    def __init__(self, base_url: str | None = None, timeout: float = 15.0) -> None:
        self.base_url = base_url or settings.gamma_api_base
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=timeout,
            headers={"User-Agent": "polymarket-sim/0.1 (educational)"},
        )

    async def fetch_active_markets(self, limit: int = 1000) -> list[dict[str, Any]]:
        """Fetch active markets. Returns raw list of dicts from Gamma."""
        url = f"/markets?limit={limit}&active=true"
        try:
            resp = await self._client.get(url)
            resp.raise_for_status()
            data = resp.json()
            if isinstance(data, list):
                return data
            logger.warning("Unexpected Gamma response shape: %s", type(data))
            return []
        except Exception as exc:
            logger.exception("Failed to fetch markets from Gamma: %s", exc)
            return []

    async def close(self) -> None:
        await self._client.aclose()
