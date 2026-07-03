"""PredLab client — trade the Polymarket-style club paper simulator.

    export POLY_API_KEY="pm_paper_..."     # key your admin gave you
    python -c "from predlab import PolymarketClient; print(PolymarketClient().markets(limit=1))"

Override POLY_BASE to point at a local sim instead of the club host.
"""

from __future__ import annotations

import os
from typing import Any

import requests

__all__ = ["PolymarketClient"]

POLY_BASE = os.environ.get("POLY_BASE", "https://poly.teddytennant.com")


class PolymarketClient:
    """Polymarket-style paper trading client. Reads public; writes need your key."""

    def __init__(self, base_url: str = POLY_BASE, api_key: str | None = None):
        self.base = base_url.rstrip("/")
        self.key = api_key or os.environ.get("POLY_API_KEY")

    def _headers(self) -> dict[str, str]:
        return {"POLY_API_KEY": self.key} if self.key else {}

    def markets(self, **params: Any) -> Any:
        """List markets. e.g. markets(limit=5)."""
        r = requests.get(f"{self.base}/markets", params=params, timeout=15)
        r.raise_for_status()
        return r.json()

    def book(self, token_id: str) -> Any:
        """Order book for one outcome token."""
        r = requests.get(f"{self.base}/book", params={"token_id": token_id}, timeout=15)
        r.raise_for_status()
        return r.json()

    def positions(self) -> Any:
        """Your positions (requires POLY_API_KEY)."""
        r = requests.get(f"{self.base}/positions", headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()

    def portfolio(self) -> Any:
        """Your cash, positions value, and net worth (requires POLY_API_KEY)."""
        r = requests.get(f"{self.base}/portfolio", headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()

    def place_order(self, token_id: str, side: str, price: float, size: float) -> Any:
        """Buy or sell `size` shares of `token_id` at `price` (0.0-1.0).
        side: "BUY" or "SELL".
        """
        body = {"token_id": token_id, "side": side.upper(), "price": price, "size": size}
        r = requests.post(f"{self.base}/order", json=body, headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()

    def cancel_order(self, order_id: int | str) -> Any:
        """Cancel one of your resting orders by id (requires POLY_API_KEY).

        Use this to clear an order that's still `open` (unfilled) - the buy
        escrow is refunded to your cash.
        """
        body = {"orderID": str(order_id)}
        r = requests.delete(f"{self.base}/order", json=body, headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()


def _smoke() -> None:
    """Quick connectivity + auth check."""
    poly = PolymarketClient()
    mkts = poly.markets(limit=1)
    print(f"reachable - sample: {str(mkts)[:80]}...")
    if poly.key:
        try:
            print(f"your positions: {poly.positions()}")
        except Exception as e:
            print(f"(positions check failed: {e})")
    else:
        print("(set POLY_API_KEY to also test authenticated calls)")


if __name__ == "__main__":
    _smoke()
