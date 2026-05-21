"""Leaderboard: members ranked by paper net worth (cash + marked positions)."""

from __future__ import annotations

from kalshi_sim.models.db import PaperAccount

GEN = "/trade-api/v2/admin/leaderboard"


def test_leaderboard_requires_admin(client):
    assert client.get(GEN).status_code == 403


def test_leaderboard_ranks_by_net_worth(client, session, make_user, admin_secret):
    rich = make_user(session, "rich")
    poor = make_user(session, "poor")
    session.add(PaperAccount(user_id=rich.id, balance_cents=9_000_000))
    session.add(PaperAccount(user_id=poor.id, balance_cents=1_000_000))
    session.commit()

    r = client.get(GEN, headers={"X-Kalshi-Sim-Admin": admin_secret})
    assert r.status_code == 200
    board = r.json()

    names = [row["username"] for row in board]
    assert names.index("rich") < names.index("poor"), "richer member ranks higher"
    nets = [row["net_worth"] for row in board]
    assert nets == sorted(nets, reverse=True), "sorted by net worth, highest first"
    assert board[0]["net_worth"] == 90000.0  # 9,000,000 cents -> dollars
    assert board[0]["cash"] == 90000.0
