"""Small shared helpers."""

from datetime import UTC, datetime


def utcnow() -> datetime:
    """Current UTC time as a naive datetime (DB timestamps are stored naive UTC)."""
    return datetime.now(UTC).replace(tzinfo=None)
