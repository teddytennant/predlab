"""SQLAlchemy 2.0 ORM models for Kalshi-sim.

Kalshi fidelity notes:
- Markets identified by `ticker` (e.g. "INFLATION-25DEC31-T1")
- Prices stored as strings (FixedPointDollars like "0.5600") to match real API exactly
- Paper accounts use cents (integer) for USD balance
- Orders reference ticker + side (yes/no) + price in cents or dollars
"""
from __future__ import annotations

import uuid
from datetime import datetime
from typing import Optional

from sqlalchemy import (
    JSON,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    String,
    Text,
    func,
)
from sqlalchemy.orm import Mapped, mapped_column, relationship

from ..db import Base


def gen_uuid() -> str:
    return str(uuid.uuid4())


class User(Base):
    """Club member / paper trader. Human-readable username for leaderboard."""

    __tablename__ = "users"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    username: Mapped[str] = mapped_column(String(64), unique=True, index=True, nullable=False)
    display_name: Mapped[Optional[str]] = mapped_column(String(128))
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    paper_account: Mapped["PaperAccount"] = relationship(
        back_populates="user", uselist=False, cascade="all, delete-orphan"
    )
    api_keys: Mapped[list["ApiKey"]] = relationship(back_populates="user", cascade="all, delete-orphan")
    orders: Mapped[list["Order"]] = relationship(back_populates="user")
    positions: Mapped[list["Position"]] = relationship(back_populates="user")
    trades: Mapped[list["Trade"]] = relationship(back_populates="user")


class PaperAccount(Base):
    """Paper money account. Balance in cents. Marked-to-market on demand."""

    __tablename__ = "paper_accounts"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    user_id: Mapped[str] = mapped_column(ForeignKey("users.id"), unique=True, nullable=False)
    balance_cents: Mapped[int] = mapped_column(Integer, default=2_500_000, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )

    user: Mapped["User"] = relationship(back_populates="paper_account")


class Market(Base):
    """Canonical synced market from Kalshi. Updated by sync service.

    Uses Kalshi `ticker` as natural primary key for perfect fidelity.
    Prices and volumes stored as string to avoid any FP drift.
    """

    __tablename__ = "markets"

    ticker: Mapped[str] = mapped_column(String(64), primary_key=True)
    event_ticker: Mapped[str] = mapped_column(String(64), index=True)
    # status also indexed via column below (see __table_args__ removed to avoid dup with SA auto-index)
    title: Mapped[Optional[str]] = mapped_column(Text)
    subtitle: Mapped[Optional[str]] = mapped_column(Text)
    yes_sub_title: Mapped[str] = mapped_column(String(256))
    no_sub_title: Mapped[str] = mapped_column(String(256))

    status: Mapped[str] = mapped_column(String(32), default="active", index=True)  # active, closed, determined, settled...
    result: Mapped[Optional[str]] = mapped_column(String(8))  # "yes" | "no" after resolution (for settlement)
    market_type: Mapped[str] = mapped_column(String(16), default="binary")

    open_time: Mapped[Optional[datetime]]
    close_time: Mapped[Optional[datetime]]
    latest_expiration_time: Mapped[Optional[datetime]]

    # Prices & volumes exactly as Kalshi returns them (string fixed-point)
    yes_bid_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")
    yes_ask_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")
    no_bid_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")
    no_ask_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")
    last_price_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")

    yes_bid_size_fp: Mapped[str] = mapped_column(String(16), default="0.00")
    yes_ask_size_fp: Mapped[str] = mapped_column(String(16), default="0.00")

    volume_fp: Mapped[str] = mapped_column(String(16), default="0.00")
    volume_24h_fp: Mapped[str] = mapped_column(String(16), default="0.00")
    open_interest_fp: Mapped[str] = mapped_column(String(16), default="0.00")
    liquidity_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")

    notional_value_dollars: Mapped[str] = mapped_column(String(16), default="1.0000")

    rules_primary: Mapped[Optional[str]] = mapped_column(Text)
    rules_secondary: Mapped[Optional[str]] = mapped_column(Text)

    # Raw upstream payload for debugging / future rehydration
    raw_json: Mapped[Optional[dict]] = mapped_column(JSON)

    last_synced_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())


class Order(Base):
    """Paper order (limit, market, etc.). Kalshi-style fields."""

    __tablename__ = "orders"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    user_id: Mapped[str] = mapped_column(ForeignKey("users.id"), nullable=False, index=True)
    ticker: Mapped[str] = mapped_column(ForeignKey("markets.ticker"), nullable=False, index=True)

    side: Mapped[str] = mapped_column(String(8), nullable=False)  # "bid" | "ask" (V2) or legacy yes/no
    action: Mapped[str] = mapped_column(String(16), default="buy")  # internal: buy/sell yes

    # V2 shape: prices and counts as strings (fixed point)
    price_dollars: Mapped[str] = mapped_column(String(16), nullable=False)
    count: Mapped[int] = mapped_column(Integer, nullable=False)  # whole contracts for sim MVP; fp support later

    client_order_id: Mapped[Optional[str]] = mapped_column(String(128), index=True)
    time_in_force: Mapped[str] = mapped_column(String(32), default="good_till_canceled")
    post_only: Mapped[bool] = mapped_column(default=False)
    reduce_only: Mapped[bool] = mapped_column(default=False)

    # Execution info
    avg_fill_price_dollars: Mapped[Optional[str]] = mapped_column(String(16))

    # Status
    status: Mapped[str] = mapped_column(String(16), default="open")  # open, filled, cancelled, partially_filled
    filled_count: Mapped[int] = mapped_column(Integer, default=0)

    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )

    user: Mapped["User"] = relationship(back_populates="orders")
    # trades via relationship later


class Trade(Base):
    """Executed fill. Linked to order + market."""

    __tablename__ = "trades"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    user_id: Mapped[str] = mapped_column(ForeignKey("users.id"), nullable=False, index=True)
    order_id: Mapped[str] = mapped_column(ForeignKey("orders.id"), nullable=False, index=True)
    ticker: Mapped[str] = mapped_column(ForeignKey("markets.ticker"), nullable=False)

    side: Mapped[str] = mapped_column(String(8), nullable=False)
    price_dollars: Mapped[str] = mapped_column(String(16), nullable=False)
    count: Mapped[int] = mapped_column(Integer, nullable=False)

    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())

    user: Mapped["User"] = relationship(back_populates="trades")


class Position(Base):
    """Current paper position per ticker per user (yes contracts and no contracts, netted)."""

    __tablename__ = "positions"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    user_id: Mapped[str] = mapped_column(ForeignKey("users.id"), nullable=False, index=True)
    ticker: Mapped[str] = mapped_column(ForeignKey("markets.ticker"), nullable=False, index=True)

    yes_contracts: Mapped[int] = mapped_column(Integer, default=0)
    no_contracts: Mapped[int] = mapped_column(Integer, default=0)  # or use signed?

    avg_price_dollars: Mapped[str] = mapped_column(String(16), default="0.0000")

    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), server_default=func.now(), onupdate=func.now()
    )

    user: Mapped["User"] = relationship(back_populates="positions")

    __table_args__ = (Index("ix_positions_user_ticker", "user_id", "ticker", unique=True),)


class ApiKey(Base):
    """Issued API key for a user. Stores the public portion + metadata.

    For Phase 1: private key returned only at generation time (stored nowhere).
    In Phase 2 we will store hashed secret or pubkey for signature verification.
    """

    __tablename__ = "api_keys"

    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=gen_uuid)
    user_id: Mapped[str] = mapped_column(ForeignKey("users.id"), nullable=False, index=True)

    key_id: Mapped[str] = mapped_column(String(64), unique=True, index=True, nullable=False)  # the "api_key_id"
    # public_key_pem or private stub only on create
    public_key_pem: Mapped[Optional[str]] = mapped_column(Text)  # stored for RSA verification
    name: Mapped[Optional[str]] = mapped_column(String(128))
    scopes: Mapped[list[str]] = mapped_column(JSON, default=list)  # ["read", "write"]

    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    last_used_at: Mapped[Optional[datetime]]

    user: Mapped["User"] = relationship(back_populates="api_keys")
