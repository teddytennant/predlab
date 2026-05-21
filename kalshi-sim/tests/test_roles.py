"""Role gating on Kalshi key issuance + market resolution.

Issuing keys now requires an admin key or the club admin secret (the master
secret authenticates as owner). Only owners can mint admin/owner keys or resolve
markets. The dev auth-bypass does NOT grant admin powers.
"""

from __future__ import annotations

GEN = "/trade-api/v2/api_keys/generate"


def test_generate_requires_admin_even_with_bypass(client):
    # Dev bypass is on in tests, but key issuance still needs admin auth.
    r = client.post(GEN, json={"name": "x"}, params={"username": "rando"})
    assert r.status_code == 403


def test_generate_bad_secret_rejected(client):
    r = client.post(
        GEN,
        json={"name": "x"},
        params={"username": "rando2"},
        headers={"X-Kalshi-Sim-Admin": "nope-wrong-secret"},
    )
    assert r.status_code == 403


def test_owner_secret_issues_member_key(client, admin_secret):
    r = client.post(
        GEN,
        json={"name": "alice-laptop"},
        params={"username": "alice"},
        headers={"X-Kalshi-Sim-Admin": admin_secret},
    )
    assert r.status_code == 200
    assert r.json()["api_key_id"].startswith("ks_live_")


def test_owner_secret_can_mint_admin_key(client, admin_secret):
    r = client.post(
        GEN,
        json={"name": "vp-laptop"},
        params={"username": "vp", "role": "admin"},
        headers={"X-Kalshi-Sim-Admin": admin_secret},
    )
    assert r.status_code == 200


def test_invalid_role_rejected(client, admin_secret):
    r = client.post(
        GEN,
        json={"name": "x"},
        params={"username": "rando3", "role": "wizard"},
        headers={"X-Kalshi-Sim-Admin": admin_secret},
    )
    assert r.status_code == 400


def test_resolve_requires_owner(client, make_market, session, admin_secret):
    make_market(session, ticker="RES-MKT", status="active")
    # No auth -> 403.
    assert client.post("/trade-api/v2/admin/resolve/RES-MKT?result=yes").status_code == 403
    # Owner secret -> 200.
    ok = client.post(
        "/trade-api/v2/admin/resolve/RES-MKT?result=yes",
        headers={"X-Kalshi-Sim-Admin": admin_secret},
    )
    assert ok.status_code == 200
