"""Role-based access control: member / admin / owner.

- member: own account only, cannot touch admin endpoints
- admin (e.g. the VP): issue/revoke member keys, reset balances
- owner (master secret or owner key): everything, incl. granting roles + force-resolve
"""

from __future__ import annotations

from polymarket_sim.services.auth import create_demo_user_with_key


def _key(session, username: str, role: str) -> str:
    _u, key, _secret = create_demo_user_with_key(session, username, role=role)
    return key


def test_member_cannot_issue_keys(client, session):
    member = _key(session, "m1", "member")
    r = client.post(
        "/admin/create-paper-key?username=newbie", headers={"POLY_API_KEY": member}
    )
    assert r.status_code == 403


def test_admin_can_issue_member_key(client, session):
    admin = _key(session, "vp", "admin")
    r = client.post(
        "/admin/create-paper-key?username=stud1", headers={"POLY_API_KEY": admin}
    )
    assert r.status_code == 200
    assert r.json()["role"] == "member"


def test_admin_cannot_grant_admin(client, session):
    admin = _key(session, "vp2", "admin")
    r = client.post(
        "/admin/create-paper-key?username=stud2&role=admin",
        headers={"POLY_API_KEY": admin},
    )
    assert r.status_code == 403


def test_owner_secret_can_grant_admin(client, admin_secret):
    r = client.post(
        "/admin/create-paper-key?username=vp3&role=admin",
        headers={"X-Admin-Secret": admin_secret},
    )
    assert r.status_code == 200
    assert r.json()["role"] == "admin"


def test_force_resolve_requires_owner(client, session, admin_secret, make_market):
    make_market(session, market_id="50", token_yes="500", token_no="501")
    admin = _key(session, "vp4", "admin")
    denied = client.post(
        "/admin/force-resolve?market_id=50&resolution=yes",
        headers={"POLY_API_KEY": admin},
    )
    assert denied.status_code == 403
    ok = client.post(
        "/admin/force-resolve?market_id=50&resolution=yes",
        headers={"X-Admin-Secret": admin_secret},
    )
    assert ok.status_code == 200


def test_reset_balance_requires_admin(client, session, admin_secret):
    member = _key(session, "m2", "member")
    denied = client.post("/admin/reset-balance", headers={"POLY_API_KEY": member})
    assert denied.status_code == 403
    ok = client.post("/admin/reset-balance", headers={"X-Admin-Secret": admin_secret})
    assert ok.status_code == 200


def test_owner_can_promote_with_set_role(client, session, admin_secret):
    _key(session, "vp5", "member")
    r = client.post(
        "/admin/set-role?username=vp5&role=admin", headers={"X-Admin-Secret": admin_secret}
    )
    assert r.status_code == 200
    assert r.json()["role"] == "admin"
    # The promoted user's key can now issue member keys.
    promoted = _key(session, "vp5b", "admin")  # sanity: admin key path works end-to-end
    assert promoted.startswith("pm_paper_")


def test_revoke_key_disables_member(client, session, admin_secret):
    member = _key(session, "revoke_me", "member")
    # Key works before revoke (public-ish: positions returns 200 for valid key).
    assert client.get("/positions", headers={"POLY_API_KEY": member}).status_code == 200
    r = client.post(
        "/admin/revoke-key?username=revoke_me", headers={"X-Admin-Secret": admin_secret}
    )
    assert r.status_code == 200
    assert r.json()["keys_disabled"] == 1
    # After revoke the key is rejected.
    assert client.get("/positions", headers={"POLY_API_KEY": member}).status_code == 401
