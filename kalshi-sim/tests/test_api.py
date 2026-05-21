"""HTTP-level tests via TestClient for the /trade-api/v2 surface."""

from __future__ import annotations

ORDERS = "/trade-api/v2/portfolio/orders"
BALANCE = "/trade-api/v2/portfolio/balance"


def test_health_ok(client):
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


def test_markets_lists_seeded_market_without_network(client, session, make_market):
    make_market(session, ticker="ABC-MKT", status="active")
    resp = client.get("/trade-api/v2/markets?status=open&limit=10")
    assert resp.status_code == 200
    tickers = [m["ticker"] for m in resp.json()["markets"]]
    assert "ABC-MKT" in tickers


def test_dev_bypass_allows_order_and_balance(client, session, make_market, starting_balance):
    make_market(session, ticker="ABC-MKT", status="active")
    # No KALSHI-ACCESS-* headers -> dev bypass resolves to a demo user.
    order = client.post(
        ORDERS,
        json={
            "ticker": "ABC-MKT",
            "client_order_id": "c1",
            "side": "bid",
            "count": "5",
            "price": "0.4000",
        },
        headers={"X-Kalshi-Sim-User": "alice"},
    )
    assert order.status_code == 200
    assert order.json()["order_id"]

    bal = client.get(BALANCE, headers={"X-Kalshi-Sim-User": "alice"})
    assert bal.status_code == 200
    # Resting bid does not debit, so balance is still the starting amount.
    assert bal.json()["balance"] == starting_balance


def test_admin_resolve_requires_secret(client, session, make_market):
    make_market(session, ticker="ABC-MKT", status="active")
    # Missing admin secret -> 403.
    denied = client.post("/trade-api/v2/admin/resolve/ABC-MKT?result=yes")
    assert denied.status_code == 403
    # Correct secret -> 200.
    ok = client.post(
        "/trade-api/v2/admin/resolve/ABC-MKT?result=yes",
        headers={"X-Kalshi-Sim-Admin": "test-admin-secret"},
    )
    assert ok.status_code == 200
    assert ok.json()["result"] == "yes"


def test_auth_enforced_when_bypass_disabled(client, monkeypatch):
    from kalshi_sim.config import get_settings

    monkeypatch.setattr(get_settings(), "dev_bypass_auth", False)
    resp = client.post(
        ORDERS,
        json={
            "ticker": "ABC-MKT",
            "client_order_id": "c1",
            "side": "bid",
            "count": "5",
            "price": "0.4000",
        },
    )
    # With the bypass off and no signed headers, the request is rejected.
    assert resp.status_code == 401
