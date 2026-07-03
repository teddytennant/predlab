"""
Configuration management using Pydantic Settings.

Loads from environment variables and .env file.
"""

from functools import lru_cache

from pydantic import model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict

# Admin-secret values that are public in the repo (code default, the
# docker-compose fallback, and both .env.example files). Booting a production
# deployment with any of these would hand owner-level access to anyone who
# reads the source, so we refuse.
_PLACEHOLDER_ADMIN_SECRETS = frozenset(
    {
        "",
        "change-me-in-prod-for-club",
        "change-me-in-prod-for-club-use-only",
        "change-me-set-in-dotenv",
        "change-me-generate-with-openssl-rand-hex-32",
    }
)


class Settings(BaseSettings):
    """Application settings loaded from env / .env with validation."""

    # Environment
    environment: str = "development"
    host: str = "0.0.0.0"
    port: int = 8001

    # Database
    database_url: str = "sqlite:///./data/polymarket.db"

    # Data sync from real Polymarket Gamma API.
    gamma_api_base: str = "https://gamma-api.polymarket.com"
    # How often the background loop re-syncs the live catalog (seconds).
    # Tighter than the original 300s so member net worth and the profile
    # charts react more quickly when upstream prices actually move.
    sync_interval_seconds: int = 60
    # Only carry markets at/above this 24h-ish dollar volume. Gamma is sorted
    # volume-descending, so we stop paging once we drop below this.
    sync_min_volume: float = 1000.0
    # Upper bound on how many markets to carry. Gamma also hard-caps paging
    # around offset 10k, so this is the practical ceiling either way.
    sync_max_markets: int = 8000
    # Gamma caps each page at 100 regardless of the limit we ask for.
    sync_page_size: int = 100
    # Politeness delay between Gamma page requests (seconds) to avoid throttling.
    sync_pace_seconds: float = 0.25
    # Hard cap on the `limit` a client may request from GET /markets.
    markets_max_limit: int = 500

    # Paper money
    starting_balance_usd: float = 25000.00

    # Admin secret for privileged ops (reset balances, force resolve) - set in .env
    admin_secret: str = "change-me-in-prod-for-club"

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    @property
    def is_dev(self) -> bool:
        return self.environment.lower() in ("development", "dev", "local")

    @model_validator(mode="after")
    def _reject_placeholder_admin_secret(self) -> "Settings":
        """Fail fast if a non-dev deployment is left on a public placeholder secret.

        The default ``admin_secret`` is committed to the repo, so any production
        deploy that forgets to set ``ADMIN_SECRET`` would otherwise accept a
        publicly-known owner credential. In dev/local we keep the convenient
        default so the stack still boots out of the box.
        """
        if not self.is_dev and self.admin_secret in _PLACEHOLDER_ADMIN_SECRETS:
            raise ValueError(
                "ADMIN_SECRET is unset or a known placeholder while ENVIRONMENT="
                f"{self.environment!r}. Set a strong secret (openssl rand -hex 32) "
                "in the environment / .env before deploying."
            )
        return self


@lru_cache
def get_settings() -> Settings:
    """Return cached settings singleton."""
    return Settings()


# Convenience for imports
settings = get_settings()
