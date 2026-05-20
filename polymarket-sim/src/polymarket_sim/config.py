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

    # Data sync from real Polymarket Gamma API
    gamma_api_base: str = "https://gamma-api.polymarket.com"
    sync_interval_seconds: int = 60

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
