"""
SQLAlchemy 2.0 database setup.

Uses sync engine + session for foundation phase (simple & reliable with SQLite).
Provides get_session dependency and init_db() for table creation.
Later can be upgraded to async with asyncpg + PostgreSQL.
"""

from __future__ import annotations

import logging
from collections.abc import Generator
from contextlib import contextmanager

from sqlalchemy import create_engine
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

from .config import settings

logger = logging.getLogger(__name__)


class Base(DeclarativeBase):
    """Base class for all SQLAlchemy models (2.0 style)."""

    pass


# Engine created once at import (sync)
engine = create_engine(
    settings.database_url,
    echo=settings.is_dev,  # log SQL in dev
    connect_args={"check_same_thread": False} if "sqlite" in settings.database_url else {},
    pool_pre_ping=True,
)

# Session factory
SessionLocal = sessionmaker(
    bind=engine,
    autoflush=False,
    autocommit=False,
    expire_on_commit=False,
)


def init_db() -> None:
    """Create all tables if they do not exist. Called at startup."""
    # Import models here to register them with Base.metadata
    from .models.db import (  # noqa: F401
        ApiKey,
        Market,
        Order,
        PaperAccount,
        Position,
        Trade,
        User,
    )

    Base.metadata.create_all(bind=engine)
    logger.info("Database tables initialized (or already exist).")


@contextmanager
def get_db_session() -> Generator[Session, None, None]:
    """Context manager for a DB session (use in services / scripts)."""
    session = SessionLocal()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()


def get_session() -> Generator[Session, None, None]:
    """FastAPI dependency: yields a session and handles commit/rollback/close."""
    session = SessionLocal()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()
