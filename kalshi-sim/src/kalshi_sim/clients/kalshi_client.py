"""Thin async client for Kalshi public market data endpoints.

Only unauthenticated read endpoints are used (public market list + single market).
Matches real base https://external-api.kalshi.com/trade-api/v2
"""
import logging
from typing import Any

import httpx

from ..config import get_settings

logger = logging.getLogger(__name__)


class KalshiClient:
    """Minimal wrapper. All methods return raw dicts or raise."""

    def __init__(self, base_url: str | None = None, timeout: float = 15.0) -> None:
        settings = get_settings()
        self.base_url = (base_url or settings.kalshi_api_base).rstrip("/")
        self.timeout = timeout
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=timeout,
            headers={"User-Agent": "kalshi-sim/0.1 (educational paper trading)"},
        )

    async def close(self) -> None:
        await self._client.aclose()

    async def get_markets(
        self,
        limit: int = 20,
        status: str = "open",
        cursor: str | None = None,
        **extra_params: Any,
    ) -> dict[str, Any]:
        """GET /markets — returns the exact {markets: [...], cursor: "..."} shape."""
        params: dict[str, Any] = {"limit": limit, "status": status}
        if cursor:
            params["cursor"] = cursor
        params.update({k: v for k, v in extra_params.items() if v is not None})

        resp = await self._client.get("/markets", params=params)
        resp.raise_for_status()
        return resp.json()

    async def get_market(self, ticker: str) -> dict[str, Any]:
        """GET /markets/{ticker} — returns {"market": {...}}"""
        resp = await self._client.get(f"/markets/{ticker}")
        resp.raise_for_status()
        return resp.json()


# Singleton factory for lifespan / services
_client_instance: KalshiClient | None = None


async def get_kalshi_client() -> KalshiClient:
    global _client_instance
    if _client_instance is None:
        _client_instance = KalshiClient()
    return _client_instance
