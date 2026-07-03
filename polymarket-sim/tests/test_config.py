"""Settings validation — production must refuse a public placeholder admin secret."""

from __future__ import annotations

import pytest
from pydantic import ValidationError

from polymarket_sim.config import Settings


@pytest.mark.parametrize(
    "placeholder",
    [
        "",
        "change-me-in-prod-for-club",
        "change-me-in-prod-for-club-use-only",
        "change-me-set-in-dotenv",
        "change-me-generate-with-openssl-rand-hex-32",
    ],
)
def test_production_rejects_placeholder_admin_secret(placeholder):
    with pytest.raises(ValidationError, match="ADMIN_SECRET"):
        Settings(environment="production", admin_secret=placeholder)


def test_production_accepts_a_real_admin_secret():
    s = Settings(environment="production", admin_secret="a-strong-random-secret")
    assert s.admin_secret == "a-strong-random-secret"
    assert not s.is_dev


def test_dev_still_boots_with_the_default_secret():
    s = Settings(environment="development", admin_secret="change-me-in-prod-for-club")
    assert s.is_dev  # convenience default tolerated only in dev/local
