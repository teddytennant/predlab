"""Leaderboard: members ranked by paper net worth (cash + marked positions)."""

from __future__ import annotations

from polymarket_sim.services.auth import create_demo_user_with_key, ensure_paper_account


def test_leaderboard_requires_admin(client):
    assert client.get("/admin/leaderboard").status_code == 403


def test_leaderboard_ranks_by_net_worth(client, session, admin_secret):
    rich, _, _ = create_demo_user_with_key(session, "rich", role="member")
    poor, _, _ = create_demo_user_with_key(session, "poor", role="member")
    ensure_paper_account(session, rich).balance_usd = 90000
    ensure_paper_account(session, poor).balance_usd = 10000
    session.commit()

    r = client.get("/admin/leaderboard", headers={"X-Admin-Secret": admin_secret})
    assert r.status_code == 200
    board = r.json()

    names = [row["username"] for row in board]
    assert names.index("rich") < names.index("poor"), "richer member ranks higher"
    nets = [row["net_worth"] for row in board]
    assert nets == sorted(nets, reverse=True), "sorted by net worth, highest first"
    assert board[0]["net_worth"] == 90000.0
    assert board[0]["cash"] == 90000.0
    assert board[0]["positions_value"] == 0.0
