"""HTTP-level tests via TestClient: health, market data, auth, admin, trading."""

from __future__ import annotations

from polymarket_sim.services.auth import create_demo_user_with_key


def test_health_ok(client):
    from polymarket_sim import __version__

    resp = client.get("/health")
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "ok"
    assert body["environment"] == "test"
    assert body["version"] == __version__


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


def test_batch_orders_requires_api_key(client):
    # POST /orders is the authenticated batch path; there is no unauthenticated
    # order route (the old no-auth "legacy" handler is gone).
    resp = client.post("/orders", json=[{"tokenId": "100", "side": "BUY", "size": 1}])
    assert resp.status_code == 401


def test_delete_order_cancels_and_refunds_over_http(client, session, make_market, starting_balance):
    make_market(session, market_id="7", token_yes="700", token_no="701")
    _user, key, _secret = create_demo_user_with_key(session, "frank", "Frank")

    placed = client.post(
        "/order",
        json={"tokenId": "700", "side": "BUY", "price": 0.40, "size": 10},
        headers={"POLY_API_KEY": key},
    )
    assert placed.status_code == 200 and placed.json()["success"] is True
    order_id = placed.json()["orderID"]

    # Escrow reserved 10 * 0.40 = 4.00 while the order rests.
    cash = client.get("/portfolio", headers={"POLY_API_KEY": key}).json()["cash"]
    assert cash == starting_balance - 4.0

    cancelled = client.request(
        "DELETE", "/order", json={"orderID": order_id}, headers={"POLY_API_KEY": key}
    )
    assert cancelled.status_code == 200
    assert cancelled.json()["status"] == "cancelled"

    cash = client.get("/portfolio", headers={"POLY_API_KEY": key}).json()["cash"]
    assert cash == starting_balance  # escrow refunded


def test_delete_order_rejects_missing_id(client, session):
    _user, key, _secret = create_demo_user_with_key(session, "gina", "Gina")
    resp = client.request("DELETE", "/order", json={}, headers={"POLY_API_KEY": key})
    assert resp.status_code == 400


def test_book_returns_clob_shape_for_unquoted_token(client):
    resp = client.get("/book", params={"token_id": "no-such-token"})
    assert resp.status_code == 200
    body = resp.json()
    assert body["asset_id"] == "no-such-token"
    assert body["bids"] == [] and body["asks"] == []
