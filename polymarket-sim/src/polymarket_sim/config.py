"""
Configuration management using Pydantic Settings.

Loads from environment variables and .env file.
Follows patterns from Principia homeschool python-backend recommendations.
"""

from functools import lru_cache

from pydantic_settings import BaseSettings, SettingsConfigDict


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

    # Future
    # redis_url: str | None = None

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    @property
    def is_dev(self) -> bool:
        return self.environment.lower() in ("development", "dev", "local")


@lru_cache
def get_settings() -> Settings:
    """Return cached settings singleton."""
    return Settings()


# Convenience for imports
settings = get_settings()
