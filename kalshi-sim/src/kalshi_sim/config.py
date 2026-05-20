"""Application configuration via Pydantic Settings (12-factor style).

Follows patterns from docs/plans/06-python-backend.md.
"""
from functools import lru_cache
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Kalshi-sim settings loaded from env / .env file.

    All fields are validated at startup.
    """

    # Environment
    environment: str = Field(default="development", description="dev|staging|production")
    port: int = Field(default=8002, ge=1024, le=65535)
    log_level: str = Field(default="INFO")

    # DB - SQLite for Phase 1 foundation; Postgres URL later
    database_url: str = Field(
        default="sqlite:///./data/kalshi_sim.db",
        description="SQLAlchemy URL. Use postgresql+psycopg://... in prod",
    )

    # Paper trading
    starting_balance_cents: int = Field(
        default=2_500_000, description="Default paper balance in cents ($25k)"
    )

    # Live data sync
    kalshi_api_base: str = Field(
        default="https://external-api.kalshi.com/trade-api/v2",
        description="Public Kalshi Trade API base (no auth for markets)",
    )
    sync_interval_seconds: int = Field(default=60, ge=30, description="Poll interval for live markets")

    # Future / admin
    club_admin_secret: str = Field(default="dev-only-change-me", min_length=8)
    # redis_url: str | None = None

    # Phase 2 dev convenience: bypass RSA signature checks (set false in prod-like tests)
    dev_bypass_auth: bool = Field(
        default=True,
        description="If true, protected endpoints accept requests without valid KALSHI-ACCESS-* headers (uses demo user)",
    )

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        case_sensitive=False,
    )


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    """Return cached singleton settings instance (safe for FastAPI Depends)."""
    return Settings()


# Convenience for scripts
def ensure_data_dir() -> Path:
    """Create the data/ directory if using local SQLite file URL."""
    data_dir = Path("data")
    data_dir.mkdir(parents=True, exist_ok=True)
    return data_dir
