#!/usr/bin/env python3
"""PredLab starter client — trade the club sims with the key your admin gave you.

Two tiny clients, one per platform:

  * ``PolymarketClient`` — auth is just your API key in a header. Dead simple.
  * ``KalshiClient``     — signs every request with your RSA private key (the
                           ``.pem`` file), exactly like the real Kalshi API.

You don't need the official SDKs; this file is the whole client. It only needs
``requests`` and ``cryptography`` (``pip install -r requirements.txt``).

Quick start (fill in what your admin gave you, then run ``python predlab.py``):

    export POLY_KEY="pm_paper_..."           # your Polymarket API key
    export KALSHI_KEY_ID="ks_live_..."       # your Kalshi key id
    export KALSHI_PEM="./you.pem"            # path to your Kalshi private key file
    python predlab.py                        # read-only smoke test

Defaults point at the club's hosted servers; override POLY_BASE / KALSHI_BASE
to hit a local copy.
"""

from __future__ import annotations

import base64
import os
import time
import uuid
from typing import Any
from urllib.parse import urlsplit

import requests
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding

POLY_BASE = os.environ.get("POLY_BASE", "https://poly.teddytennant.com")
KALSHI_BASE = os.environ.get("KALSHI_BASE", "https://kalshi.teddytennant.com/trade-api/v2")


class PolymarketClient:
    """Polymarket-style sim. Reads are public; trading needs your API key."""

    def __init__(self, base_url: str = POLY_BASE, api_key: str | None = None):
        self.base = base_url.rstrip("/")
        self.key = api_key

    def _headers(self) -> dict[str, str]:
        return {"POLY_API_KEY": self.key} if self.key else {}

    def markets(self, **params: Any) -> Any:
        """Public market list. e.g. markets(limit=5)."""
        return requests.get(f"{self.base}/markets", params=params, timeout=15).json()

    def book(self, token_id: str) -> Any:
        """Public order book for one outcome token."""
        return requests.get(f"{self.base}/book", params={"token_id": token_id}, timeout=15).json()

    def positions(self) -> Any:
        """Your open positions (needs your key)."""
        r = requests.get(f"{self.base}/positions", headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()

    def place_order(self, token_id: str, side: str, price: float, size: float) -> Any:
        """Buy/sell `size` shares of `token_id` at `price` (0–1). side: BUY or SELL."""
        body = {"token_id": token_id, "side": side.upper(), "price": price, "size": size}
        r = requests.post(f"{self.base}/order", json=body, headers=self._headers(), timeout=15)
        r.raise_for_status()
        return r.json()


class KalshiClient:
    """Kalshi-style sim. Every request is RSA-PSS signed with your private key."""

    def __init__(self, key_id: str, private_key_pem_path: str, base_url: str = KALSHI_BASE):
        self.base = base_url.rstrip("/")
        self.key_id = key_id
        with open(private_key_pem_path, "rb") as f:
            self.private_key = serialization.load_pem_private_key(f.read(), password=None)

    def _signed_headers(self, method: str, path: str) -> dict[str, str]:
        # Kalshi signs the string "<ms-timestamp><METHOD><path>" (path only, no query).
        ts = str(int(time.time() * 1000))
        signature = self.private_key.sign(
            f"{ts}{method}{path}".encode(),
            padding.PSS(mgf=padding.MGF1(hashes.SHA256()), salt_length=padding.PSS.DIGEST_LENGTH),
            hashes.SHA256(),
        )
        return {
            "KALSHI-ACCESS-KEY": self.key_id,
            "KALSHI-ACCESS-TIMESTAMP": ts,
            "KALSHI-ACCESS-SIGNATURE": base64.b64encode(signature).decode(),
        }

    def _request(self, method: str, path: str, *, params: Any = None, json: Any = None) -> Any:
        url = f"{self.base}{path}"
        headers = self._signed_headers(method, urlsplit(url).path)
        r = requests.request(method, url, params=params, json=json, headers=headers, timeout=15)
        r.raise_for_status()
        return r.json()

    def balance(self) -> Any:
        return self._request("GET", "/portfolio/balance")

    def positions(self) -> Any:
        return self._request("GET", "/portfolio/positions")

    def markets(self, **params: Any) -> Any:
        return self._request("GET", "/markets", params=params)

    def create_order(self, ticker: str, side: str, count: int, price: float) -> Any:
        """Place an order. side: "bid" = buy YES, "ask" = sell YES. price in dollars (0–1)."""
        body = {
            "ticker": ticker,
            "client_order_id": str(uuid.uuid4()),
            "side": side,
            "count": str(count),
            "price": f"{price:.4f}",
        }
        return self._request("POST", "/portfolio/orders", json=body)


def _smoke() -> None:
    """Read-only check that your credentials work end-to-end."""
    poly = PolymarketClient(api_key=os.environ.get("POLY_KEY"))
    mkts = poly.markets(limit=1)
    print(f"✅ Polymarket reachable — sample market: {str(mkts)[:90]}…")
    if poly.key:
        print(f"   your Polymarket positions: {poly.positions()}")
    else:
        print("   (set POLY_KEY to check your Polymarket account)")

    key_id, pem = os.environ.get("KALSHI_KEY_ID"), os.environ.get("KALSHI_PEM")
    if key_id and pem:
        kal = KalshiClient(key_id, pem)
        print(f"✅ Kalshi signed request works — your balance: {kal.balance()}")
    else:
        print("   (set KALSHI_KEY_ID + KALSHI_PEM to check your Kalshi account)")


if __name__ == "__main__":
    _smoke()
