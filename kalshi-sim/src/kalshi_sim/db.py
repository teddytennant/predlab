"""SQLAlchemy 2.0 engine, session factory, and Base for Kalshi-sim.

Models are in models/db.py. Use get_db() dependency in routers.
Foundation uses SQLite; production will use Postgres + Alembic.
"""
import logging
from contextlib import contextmanager
from typing import Iterator

from sqlalchemy import create_engine
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

from .config import get_settings

logger = logging.getLogger(__name__)


class Base(DeclarativeBase):
    """Base class for all SQLAlchemy models. (2.0 style)"""
    pass


def get_engine():
    """Create SQLAlchemy engine from settings (cached via module)."""
    settings = get_settings()
    # echo=False in prod; True only for deep debugging
    connect_args = {"check_same_thread": False} if settings.database_url.startswith("sqlite") else {}
    engine = create_engine(
        settings.database_url,
        echo=settings.environment == "development" and False,  # set True manually when needed
        pool_pre_ping=True,
        connect_args=connect_args,
    )
    return engine


# Global engine + sessionmaker (simple for foundation)
_engine = None
_SessionLocal = None


def init_db() -> None:
    """Initialize engine and create all tables (idempotent for SQLite dev).

    For local SQLite we delete the file on every startup so we never fight
    with stale index definitions. The sync service repopulates everything
    from the live Kalshi API in < 2 seconds.
    """
    global _engine, _SessionLocal

    settings = get_settings()
    if settings.database_url.startswith("sqlite"):
        # Remove the file completely before touching the engine
        from pathlib import Path
        db_path = settings.database_url.replace("sqlite:///", "").replace("sqlite://", "")
        if db_path and db_path != ":memory:":
            Path(db_path).unlink(missing_ok=True)
            logger.info(f"Removed SQLite file for clean dev start: {db_path}")

    _engine = get_engine()
    _SessionLocal = sessionmaker(autocommit=False, autoflush=False, bind=_engine)

    # Import all models so metadata is populated
    from .models.db import (  # noqa: F401
        ApiKey,
        Market,
        Order,
        PaperAccount,
        Position,
        Trade,
        User,
    )

    Base.metadata.create_all(bind=_engine)
    _ensure_role_column(_engine)
    logger.info("Database tables ensured (clean SQLite dev mode)")


def _ensure_role_column(engine) -> None:
    """Lightweight migration: add users.role to pre-existing databases.

    create_all() never alters an existing table, so a DB created before roles
    existed would be missing the column. Idempotent across Postgres and SQLite.
    """
    from sqlalchemy import inspect, text

    insp = inspect(engine)
    if "users" not in insp.get_table_names():
        return
    cols = {c["name"] for c in insp.get_columns("users")}
    if "role" not in cols:
        with engine.begin() as conn:
            conn.execute(
                text("ALTER TABLE users ADD COLUMN role VARCHAR(16) NOT NULL DEFAULT 'member'")
            )
        logger.info("Migrated users table: added role column (default 'member').")


def get_db() -> Iterator[Session]:
    """FastAPI dependency: yields a DB session, closes after request."""
    global _SessionLocal
    if _SessionLocal is None:
        init_db()
    db = _SessionLocal()
    try:
        yield db
    finally:
        db.close()


@contextmanager
def db_session() -> Iterator[Session]:
    """Context manager for scripts / services (non-FastAPI use)."""
    global _SessionLocal
    if _SessionLocal is None:
        init_db()
    session = _SessionLocal()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
