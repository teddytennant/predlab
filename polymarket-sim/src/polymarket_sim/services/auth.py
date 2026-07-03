"""
Authentication service for the Polymarket paper trading simulator.

- API keys are bearer tokens: the key string is looked up in the ApiKey table
  (key_prefix column) and must be active. No L2/HMAC signature is verified —
  official py-clob-client works against the sim with a paper key + dummy creds.
- Role hierarchy (member < admin < owner) gates the /admin endpoints; the
  master X-Admin-Secret authenticates as owner (bootstrap / break-glass).

Headers accepted, in priority order: POLY_API_KEY, Authorization: Bearer,
X-API-Key. All paper keys are issued with the "pm_paper_" prefix.
"""

from __future__ import annotations

import hashlib
import logging
import secrets
from collections.abc import Callable
from dataclasses import dataclass

from fastapi import Depends, Header, HTTPException, status
from sqlalchemy import select
from sqlalchemy.orm import Session

from ..config import get_settings
from ..db import get_session
from ..models.db import ApiKey, PaperAccount, User
from ..util import utcnow

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


def validate_paper_api_key(db: Session, api_key: str) -> User | None:
    """Look up an incoming API key and return the owning User, or None.

    The key is a bearer token: possession of an active key row is the whole
    check. The stored secret_hash is never verified against a signature.
    """
    if not api_key:
        return None

    stmt = select(ApiKey).where(ApiKey.key_prefix == api_key, ApiKey.is_active)
    api_key_row = db.execute(stmt).scalar_one_or_none()
    if not api_key_row:
        logger.warning("Unknown or inactive API key prefix attempted: %s...", api_key[:12])
        return None

    api_key_row.last_used_at = utcnow()
    db.commit()

    return db.get(User, api_key_row.user_id)


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


# Role hierarchy. Higher rank ⊇ lower rank's powers.
ROLE_RANK: dict[str, int] = {"member": 0, "admin": 1, "owner": 2}
VALID_ROLES = frozenset(ROLE_RANK)


@dataclass
class Principal:
    """The authenticated caller of an admin-gated endpoint."""

    rank: int
    user: User | None  # None when authenticated via the master secret (owner)


def _resolve_principal(
    session: Session, x_admin_secret: str | None, api_key: str | None
) -> Principal:
    """Resolve the caller's effective role rank.

    The master secret authenticates as ``owner`` (bootstrap / break-glass). Otherwise a
    valid paper key authenticates as its user's role. Unauthenticated → rank ``-1``.
    """
    if x_admin_secret and secrets.compare_digest(x_admin_secret, settings.admin_secret):
        return Principal(rank=ROLE_RANK["owner"], user=None)
    if api_key:
        user = validate_paper_api_key(session, api_key)
        if user:
            return Principal(rank=ROLE_RANK.get(user.role, 0), user=user)
    return Principal(rank=-1, user=None)


def require_role(min_role: str) -> Callable[..., Principal]:
    """Dependency factory: require at least ``min_role`` (master secret counts as owner)."""
    min_rank = ROLE_RANK[min_role]

    def _dep(
        x_admin_secret: str | None = Header(default=None, alias="X-Admin-Secret"),
        api_key: str | None = Depends(get_api_key_from_headers),
        session: Session = Depends(get_session),
    ) -> Principal:
        principal = _resolve_principal(session, x_admin_secret, api_key)
        if principal.rank < min_rank:
            raise HTTPException(
                status_code=status.HTTP_403_FORBIDDEN,
                detail=f"requires '{min_role}' role (an admin key or the X-Admin-Secret)",
            )
        return principal

    return _dep


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
    db: Session, username: str, display_name: str | None = None, role: str = "member"
) -> tuple[User, str, str]:
    """
    Admin helper: create User + PaperAccount + ApiKey (paper).
    Returns (user, key_prefix, plain_secret_for_user)
    The plain_secret should be shown only once (like real CLOB key creation).
    """
    if role not in VALID_ROLES:
        raise ValueError(f"invalid role {role!r} (one of {sorted(VALID_ROLES)})")

    # Unique username
    existing = db.execute(select(User).where(User.username == username)).scalar_one_or_none()
    if existing:
        raise ValueError(f"User {username} already exists")

    user = User(username=username, display_name=display_name or username, role=role)
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
