"""
Authentication service for the Polymarket paper trading simulator (Phase 2).

- API key validation using the ApiKey model (key_prefix lookup).
- Paper-mode bypass: for keys with "pm_paper_" prefix (issued by us), we accept the
  POLY_API_KEY header value directly without requiring valid L2 HMAC signature.
  This allows official py-clob-client (or raw httpx) to target the sim with a
  generated paper key + dummy creds (sig is ignored on server for paper keys).
- Real L2 sig verification can be added later behind a flag (would require storing
  plaintext secret or re-deriving).
- Admin secret gate for privileged endpoints (reset, force-resolve).
- Dependency helpers for FastAPI routes: get_current_user, require_admin.

Headers supported for compatibility:
- POLY_API_KEY (primary, from clob-client L2)
- Authorization: Bearer <key>
- Fallback: X-API-Key

All paper keys are issued with prefix "pm_paper_..." so they are easily identified.
"""

from __future__ import annotations

import hashlib
import logging
import secrets

from fastapi import Depends, Header, HTTPException, status
from sqlalchemy import select
from sqlalchemy.orm import Session

from ..config import get_settings
from ..db import get_session
from ..models.db import ApiKey, PaperAccount, User

logger = logging.getLogger(__name__)
settings = get_settings()


def _hash_secret(secret: str) -> str:
    """Store only hash of the secret (never plaintext)."""
    return hashlib.sha256(secret.encode("utf-8")).hexdigest()


def generate_paper_api_key() -> tuple[str, str, str]:
    """
    Generate a new paper API key for a user.
    Returns (key_prefix, secret_plain, secret_hash).
    The key_prefix is what goes into POLY_API_KEY header.
    The plain secret is shown ONCE to the human (like real platforms); we only store hash.
    """
    prefix = f"pm_paper_{secrets.token_urlsafe(12)}"
    secret = secrets.token_urlsafe(32)
    secret_hash = _hash_secret(secret)
    return prefix, secret, secret_hash


def validate_paper_api_key(
    db: Session,
    api_key: str,
    passphrase: str | None = None,  # accepted for compat but ignored for paper
) -> User | None:
    """
    Validate an incoming API key (from POLY_API_KEY header etc).
    For paper-prefixed keys: just presence check (bypass signature verification).
    Returns the owning User or None.
    """
    if not api_key:
        return None

    stmt = select(ApiKey).where(ApiKey.key_prefix == api_key, ApiKey.is_active)
    api_key_row = db.execute(stmt).scalar_one_or_none()
    if not api_key_row:
        logger.warning("Unknown or inactive API key prefix attempted: %s...", api_key[:12])
        return None

    # For paper keys we deliberately skip secret/signature check.
    # (In real mode we would verify HMAC using stored hash + provided timestamp/sig.)
    if api_key.startswith("pm_paper_"):
        logger.info("Paper key authenticated for user_id=%s", api_key_row.user_id)
    else:
        # Placeholder: in future full L2 mode we would validate here using secret_hash
        logger.info(
            "Non-paper key seen (future full sig validation) user_id=%s", api_key_row.user_id
        )

    # Touch last_used
    from datetime import datetime

    api_key_row.last_used_at = datetime.utcnow()
    db.commit()

    user = db.get(User, api_key_row.user_id)
    return user


def get_api_key_from_headers(
    poly_api_key: str | None = Header(default=None, alias="POLY_API_KEY"),
    authorization: str | None = Header(default=None),
    x_api_key: str | None = Header(default=None, alias="X-API-Key"),
) -> str | None:
    """Extract the api key string from common Polymarket / generic headers."""
    if poly_api_key:
        return poly_api_key.strip()
    if authorization and authorization.lower().startswith("bearer "):
        return authorization.split(" ", 1)[1].strip()
    if x_api_key:
        return x_api_key.strip()
    return None


def get_current_user(
    api_key: str | None = Depends(get_api_key_from_headers),
    session: Session = Depends(get_session),
) -> User:
    """FastAPI dependency: returns the authenticated User or raises 401."""
    if not api_key:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Missing API key (POLY_API_KEY header or Authorization: Bearer)",
        )

    user = validate_paper_api_key(session, api_key)
    if not user:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid or inactive paper API key",
        )
    return user


def require_admin(
    x_admin_secret: str | None = Header(default=None, alias="X-Admin-Secret"),
) -> None:
    """FastAPI dependency for admin-only endpoints. Raises 403 on mismatch."""
    if not x_admin_secret or x_admin_secret != settings.admin_secret:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Invalid admin secret (X-Admin-Secret header)",
        )


def ensure_paper_account(db: Session, user: User) -> PaperAccount:
    """Idempotent: get or create the user's paper account with starting balance."""
    acct = db.execute(
        select(PaperAccount).where(PaperAccount.user_id == user.id)
    ).scalar_one_or_none()
    if acct:
        return acct

    acct = PaperAccount(user_id=user.id, balance_usd=settings.starting_balance_usd)
    db.add(acct)
    db.flush()
    logger.info(
        "Created paper account for user %s with starting balance %.2f",
        user.username,
        settings.starting_balance_usd,
    )
    return acct


def create_demo_user_with_key(
    db: Session, username: str, display_name: str | None = None
) -> tuple[User, str, str]:
    """
    Admin helper: create User + PaperAccount + ApiKey (paper).
    Returns (user, key_prefix, plain_secret_for_user)
    The plain_secret should be shown only once (like real CLOB key creation).
    """
    # Unique username
    existing = db.execute(select(User).where(User.username == username)).scalar_one_or_none()
    if existing:
        raise ValueError(f"User {username} already exists")

    user = User(username=username, display_name=display_name or username)
    db.add(user)
    db.flush()

    # Ensure paper account
    ensure_paper_account(db, user)

    # Issue paper key
    key_prefix, secret_plain, secret_hash = generate_paper_api_key()
    api_key = ApiKey(
        user_id=user.id,
        key_prefix=key_prefix,
        secret_hash=secret_hash,
        label=f"paper-key for {username}",
    )
    db.add(api_key)
    db.commit()
    db.refresh(user)

    logger.info("Demo user created: %s with paper key prefix %s", username, key_prefix)
    return user, key_prefix, secret_plain
