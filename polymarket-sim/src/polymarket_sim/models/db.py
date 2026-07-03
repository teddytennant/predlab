"""
SQLAlchemy 2.0 ORM models for the Polymarket paper trading simulator.

Models:
- User: human / bot identity (username for leaderboard)
- ApiKey: long-lived keys issued by admin (hashed secret)
- PaperAccount: one per user, holds virtual USD balance
- Market: canonical live market synced from Gamma (stores current prices)
- Order: paper limit/market orders placed against the sim
- Trade: fills / executions
- Position: current holdings per outcome token for a user
- NetWorthSnapshot: points on each user's net-worth-over-time curve

Timestamps are stored as naive UTC.
"""

from __future__ import annotations

from datetime import datetime
from decimal import Decimal

from sqlalchemy import (
    JSON,
    Boolean,
    DateTime,
    ForeignKey,
    Index,
    Integer,
    Numeric,
    String,
    UniqueConstraint,
    func,
)
from sqlalchemy.orm import Mapped, mapped_column, relationship

from ..db import Base


class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    username: Mapped[str] = mapped_column(String(64), unique=True, nullable=False, index=True)
    display_name: Mapped[str | None] = mapped_column(String(128))
    # Access role: "member" (own account only), "admin" (issue/revoke keys,
    # reset balances), or "owner" (everything, incl. force-resolve markets).
    role: Mapped[str] = mapped_column(
        String(16), nullable=False, default="member", server_default="member"
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )

    api_keys: Mapped[list[ApiKey]] = relationship(
        back_populates="user", cascade="all, delete-orphan"
    )
    paper_account: Mapped[PaperAccount | None] = relationship(
        back_populates="user", uselist=False, cascade="all, delete-orphan"
    )
    orders: Mapped[list[Order]] = relationship(back_populates="user")
    positions: Mapped[list[Position]] = relationship(back_populates="user")
    trades: Mapped[list[Trade]] = relationship(back_populates="user")


class ApiKey(Base):
    __tablename__ = "api_keys"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id", ondelete="CASCADE"), nullable=False)
    key_prefix: Mapped[str] = mapped_column(String(32), nullable=False)  # e.g. "pm_paper_xxxx"
    secret_hash: Mapped[str] = mapped_column(String(128), nullable=False)  # store only hash
    label: Mapped[str | None] = mapped_column(String(128))
    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )
    last_used_at: Mapped[datetime | None] = mapped_column(DateTime)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)

    user: Mapped[User] = relationship(back_populates="api_keys")

    __table_args__ = (UniqueConstraint("key_prefix", name="uq_api_key_prefix"),)


class PaperAccount(Base):
    __tablename__ = "paper_accounts"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    user_id: Mapped[int] = mapped_column(
        ForeignKey("users.id", ondelete="CASCADE"), unique=True, nullable=False
    )
    balance_usd: Mapped[Decimal] = mapped_column(Numeric(18, 6), default=0, nullable=False)
    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now(), nullable=False
    )

    user: Mapped[User] = relationship(back_populates="paper_account")


class Market(Base):
    """Canonical market record synced from real Polymarket Gamma API.

    Stores current best bid/ask, outcome prices, etc. for paper trading reference.
    """

    __tablename__ = "markets"

    id: Mapped[str] = mapped_column(String(32), primary_key=True)  # gamma id
    condition_id: Mapped[str] = mapped_column(String(66), unique=True, nullable=False, index=True)
    question: Mapped[str] = mapped_column(String(512), nullable=False)
    slug: Mapped[str] = mapped_column(String(128), nullable=False, index=True)

    outcomes: Mapped[list[str]] = mapped_column(JSON, nullable=False)  # ["Yes", "No"]
    outcome_prices: Mapped[list[str]] = mapped_column(JSON, nullable=False)  # ["0.51", "0.49"]
    clob_token_ids: Mapped[list[str] | None] = mapped_column(JSON)  # two token ids for CLOB

    best_bid: Mapped[float | None] = mapped_column(Numeric(10, 6))
    best_ask: Mapped[float | None] = mapped_column(Numeric(10, 6))
    last_trade_price: Mapped[float | None] = mapped_column(Numeric(10, 6))
    spread: Mapped[float | None] = mapped_column(Numeric(10, 6))

    volume: Mapped[float | None] = mapped_column(Numeric(18, 4))
    liquidity: Mapped[float | None] = mapped_column(Numeric(18, 4))

    active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)
    closed: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)

    start_date: Mapped[datetime | None] = mapped_column(DateTime)
    end_date: Mapped[datetime | None] = mapped_column(DateTime)
    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now(), nullable=False
    )
    last_synced_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )

    orders: Mapped[list[Order]] = relationship(back_populates="market")

    __table_args__ = (Index("ix_markets_active_closed", "active", "closed"),)


class Order(Base):
    """Paper order placed by a user in the simulator."""

    __tablename__ = "orders"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), nullable=False)
    market_id: Mapped[str] = mapped_column(ForeignKey("markets.id"), nullable=False)
    clob_token_id: Mapped[str | None] = mapped_column(String(80))  # which outcome leg

    side: Mapped[str] = mapped_column(String(4), nullable=False)  # "buy" | "sell"
    order_type: Mapped[str] = mapped_column(
        String(10), default="limit", nullable=False
    )  # limit/market

    price: Mapped[float | None] = mapped_column(Numeric(10, 6))  # limit price (None for market)
    size: Mapped[float] = mapped_column(Numeric(18, 6), nullable=False)  # shares requested

    filled_size: Mapped[float] = mapped_column(Numeric(18, 6), default=0.0, nullable=False)
    status: Mapped[str] = mapped_column(
        String(12), default="open", nullable=False
    )  # open/filled/cancelled/partial

    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now(), nullable=False
    )

    user: Mapped[User] = relationship(back_populates="orders")
    market: Mapped[Market] = relationship(back_populates="orders")
    trades: Mapped[list[Trade]] = relationship(back_populates="order")

    __table_args__ = (
        Index("ix_orders_user_status", "user_id", "status"),
        Index("ix_orders_market_status", "market_id", "status"),
    )


class Trade(Base):
    """Execution / fill record."""

    __tablename__ = "trades"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    order_id: Mapped[int] = mapped_column(ForeignKey("orders.id"), nullable=False)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), nullable=False)
    market_id: Mapped[str] = mapped_column(ForeignKey("markets.id"), nullable=False)
    clob_token_id: Mapped[str | None] = mapped_column(String(80))

    price: Mapped[float] = mapped_column(Numeric(10, 6), nullable=False)
    size: Mapped[float] = mapped_column(Numeric(18, 6), nullable=False)
    side: Mapped[str] = mapped_column(String(4), nullable=False)

    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )

    order: Mapped[Order] = relationship(back_populates="trades")
    user: Mapped[User] = relationship(back_populates="trades")


class Position(Base):
    """Current paper position in a specific outcome token for a user.

    Marked-to-market using latest Market prices.
    """

    __tablename__ = "positions"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(ForeignKey("users.id"), nullable=False)
    market_id: Mapped[str] = mapped_column(ForeignKey("markets.id"), nullable=False)
    clob_token_id: Mapped[str] = mapped_column(String(80), nullable=False)

    size: Mapped[float] = mapped_column(
        Numeric(18, 6), default=0.0, nullable=False
    )  # positive = long
    avg_entry_price: Mapped[float | None] = mapped_column(Numeric(10, 6))

    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False
    )
    updated_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), onupdate=func.now(), nullable=False
    )

    user: Mapped[User] = relationship(back_populates="positions")

    __table_args__ = (
        UniqueConstraint(
            "user_id", "market_id", "clob_token_id", name="uq_position_user_market_token"
        ),
        Index("ix_positions_user", "user_id"),
    )


class NetWorthSnapshot(Base):
    """A point on a user's net-worth-over-time curve (for the profile graph).

    Recorded on a periodic tick and after account-changing events (fills,
    settlement, reset). Mark-to-market net worth = cash + escrow + positions.
    """

    __tablename__ = "net_worth_snapshots"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    user_id: Mapped[int] = mapped_column(
        ForeignKey("users.id", ondelete="CASCADE"), nullable=False
    )

    net_worth: Mapped[float] = mapped_column(Numeric(18, 6), nullable=False)
    cash: Mapped[float] = mapped_column(Numeric(18, 6), nullable=False)
    positions_value: Mapped[float] = mapped_column(Numeric(18, 6), nullable=False)

    created_at: Mapped[datetime] = mapped_column(
        DateTime, server_default=func.now(), nullable=False, index=True
    )

    __table_args__ = (Index("ix_snapshots_user_time", "user_id", "created_at"),)
