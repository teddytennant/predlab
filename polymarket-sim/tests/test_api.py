"""HTTP-level tests via TestClient: health, market data, auth, admin, trading."""

from __future__ import annotations

from polymarket_sim.services.auth import create_demo_user_with_key


def test_health_ok(client):
    resp = client.get("/health")
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "ok"
    assert body["environment"] == "test"


def test_markets_returns_seeded_market(client, session, make_market):
    make_market(session, market_id="42", token_yes="900", token_no="901")
    resp = client.get("/markets?active=true&limit=10")
    assert resp.status_code == 200
    data = resp.json()
    assert any(m["id"] == "42" for m in data)


def test_trading_requires_api_key(client):
    # No POLY_API_KEY header -> 401.
    resp = client.post("/order", json={"tokenId": "100", "side": "BUY", "price": 0.5, "size": 1})
    assert resp.status_code == 401


def test_invalid_api_key_rejected(client):
    resp = client.post(
        "/order",
        json={"tokenId": "100", "side": "BUY", "price": 0.5, "size": 1},
        headers={"POLY_API_KEY": "pm_paper_does_not_exist"},
    )
    assert resp.status_code == 401


def test_admin_create_key_requires_secret(client):
    resp = client.post("/admin/create-paper-key?username=bob")
    assert resp.status_code == 403


def test_admin_create_key_and_place_order(client, session, make_market, admin_secret):
    make_market(session, market_id="1", token_yes="100", token_no="101")

    # Admin issues a paper key.
    created = client.post(
        "/admin/create-paper-key?username=carol&display_name=Carol",
        headers={"X-Admin-Secret": admin_secret},
    )
    assert created.status_code == 200
    key = created.json()["api_key"]
    assert key.startswith("pm_paper_")

    # That key can place an order.
    resp = client.post(
        "/order",
        json={"tokenId": "100", "side": "BUY", "price": 0.5, "size": 2},
        headers={"POLY_API_KEY": key},
    )
    assert resp.status_code == 200
    body = resp.json()
    assert body["success"] is True
    assert body["status"] in {"open", "filled", "partial"}


def test_positions_endpoint_authenticated(client, session, market):
    user, key, _secret = create_demo_user_with_key(session, "dave", "Dave")
    resp = client.get("/positions", headers={"POLY_API_KEY": key})
    assert resp.status_code == 200
    assert resp.json() == []  # no positions yet


def test_portfolio_requires_api_key(client):
    resp = client.get("/portfolio")
    assert resp.status_code == 401


def test_portfolio_summary_starts_at_starting_balance(client, session, starting_balance):
    _user, key, _secret = create_demo_user_with_key(session, "erin", "Erin")
    resp = client.get("/portfolio", headers={"POLY_API_KEY": key})
    assert resp.status_code == 200
    body = resp.json()
    assert body["cash"] == starting_balance
    assert body["positions_value"] == 0
    assert body["net_worth"] == starting_balance
