"""Tests for Gamma volume-descending pagination and the /markets limit clamp."""

from __future__ import annotations

import asyncio

from polymarket_sim.clients.gamma import GammaClient

PAGE_SIZE = 100


def _page(n: int, volume: float) -> list[dict]:
    return [{"id": f"{volume}-{i}", "volume": volume} for i in range(n)]


def _fetch(pages: list[list[dict]], *, min_volume: float, max_markets: int):
    """Drive fetch_markets_by_volume against canned pages.

    Returns (collected_markets, number_of_page_requests).
    """
    client = GammaClient()
    requested: list[int] = []

    async def fake_get_page(offset: int, size: int) -> list[dict]:
        requested.append(offset)
        idx = offset // PAGE_SIZE
        return pages[idx] if idx < len(pages) else []

    client._get_page = fake_get_page  # type: ignore[method-assign]

    async def run():
        try:
            return await client.fetch_markets_by_volume(
                min_volume=min_volume,
                max_markets=max_markets,
                page_size=PAGE_SIZE,
                pace_seconds=0,
            )
        finally:
            await client.close()

    got = asyncio.run(run())
    return got, len(requested)


def test_pagination_stops_at_volume_floor():
    # page0: 100 @ $2000; page1: 50 @ $2000 then 50 @ $500 (below the floor)
    pages = [_page(100, 2000), _page(50, 2000) + _page(50, 500)]
    got, n = _fetch(pages, min_volume=1000, max_markets=10_000)
    assert len(got) == 150  # 100 + the 50 above the floor
    assert n == 2  # bailed mid-page-1, never asked for page 2
    assert all(float(m["volume"]) >= 1000 for m in got)


def test_pagination_stops_at_max_markets():
    pages = [_page(100, 2000), _page(100, 2000), _page(100, 2000)]
    got, n = _fetch(pages, min_volume=0, max_markets=120)
    assert len(got) == 120
    assert n == 2  # stopped partway through page 1, never asked for page 2


def test_pagination_stops_on_short_page():
    pages = [_page(100, 2000), _page(40, 2000)]  # short page => end of data
    got, n = _fetch(pages, min_volume=0, max_markets=10_000)
    assert len(got) == 140
    assert n == 2


def test_pagination_stops_on_empty_or_error_page():
    # page 1 comes back empty (simulates Gamma's 422 past the offset cap).
    pages = [_page(100, 2000)]
    got, n = _fetch(pages, min_volume=0, max_markets=10_000)
    assert len(got) == 100
    assert n == 2  # asked for page 1, got nothing, stopped cleanly


def test_markets_endpoint_clamps_limit(client, session, make_market, monkeypatch):
    from polymarket_sim.config import settings

    monkeypatch.setattr(settings, "markets_max_limit", 2)
    for i in range(5):
        make_market(
            session, market_id=str(900 + i), token_yes=str(9000 + i), token_no=str(9500 + i)
        )
    resp = client.get("/markets?limit=100")
    assert resp.status_code == 200
    assert len(resp.json()) == 2  # clamped to markets_max_limit, not all 5
