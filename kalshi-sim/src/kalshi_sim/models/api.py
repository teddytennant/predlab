"""Pydantic v2 models for Kalshi Trade API v2 request/response shapes.

These mirror the real OpenAPI as closely as possible for drop-in SDK compatibility.
See https://docs.kalshi.com for authoritative schemas.
"""
from __future__ import annotations

from datetime import datetime
from typing import Optional

from pydantic import BaseModel, Field

# --- Market shapes (exact fidelity for /trade-api/v2/markets) ---

class PriceRange(BaseModel):
    start: str
    end: str
    step: str


class MarketResponse(BaseModel):
    """Single market object as returned by Kalshi GET /markets and /markets/{ticker}."""

    ticker: str
    event_ticker: str
    market_type: str = "binary"
    title: Optional[str] = None
    subtitle: Optional[str] = None
    yes_sub_title: str
    no_sub_title: str

    created_time: Optional[datetime] = None
    updated_time: Optional[datetime] = None
    open_time: Optional[datetime] = None
    close_time: Optional[datetime] = None
    latest_expiration_time: Optional[datetime] = None

    status: str
    yes_bid_dollars: str = "0.0000"
    yes_ask_dollars: str = "0.0000"
    no_bid_dollars: str = "0.0000"
    no_ask_dollars: str = "0.0000"
    yes_bid_size_fp: str = "0.00"
    yes_ask_size_fp: str = "0.00"
    last_price_dollars: str = "0.0000"
    previous_yes_bid_dollars: str = "0.0000"
    previous_yes_ask_dollars: str = "0.0000"
    previous_price_dollars: str = "0.0000"

    volume_fp: str = "0.00"
    volume_24h_fp: str = "0.00"
    open_interest_fp: str = "0.00"
    notional_value_dollars: str = "1.0000"
    liquidity_dollars: str = "0.0000"

    result: str = ""
    can_close_early: bool = False
    fractional_trading_enabled: bool = True
    is_provisional: bool = False

    rules_primary: Optional[str] = None
    rules_secondary: Optional[str] = None
    price_level_structure: Optional[str] = None
    price_ranges: list[PriceRange] = Field(default_factory=list)

    # Extra sim-only flag for clarity in responses (can be stripped later)
    simulated: bool = True


class GetMarketsResponse(BaseModel):
    """Exact shape of Kalshi GET /trade-api/v2/markets response."""

    markets: list[MarketResponse]
    cursor: str = ""


# --- Order V2 shapes (preferred for /portfolio/orders and SDK compatibility) ---
# Matches Kalshi CreateOrderV2Request / CreateOrderV2Response closely

class CreateOrderRequestV2(BaseModel):
    """V2 request body for POST /trade-api/v2/portfolio/orders (and /events/orders alias).

    Uses bid/ask side vocabulary, string fixed-point for price and count.
    client_order_id for idempotency.
    """

    ticker: str
    client_order_id: str
    side: str = Field(..., pattern="^(bid|ask)$")  # bid = buy YES, ask = sell YES
    count: str  # e.g. "10.00" or "5"
    price: str  # e.g. "0.6500"
    time_in_force: str = Field(default="good_till_canceled", pattern="^(fill_or_kill|good_till_canceled|immediate_or_cancel)$")
    self_trade_prevention_type: str = Field(default="taker_at_cross", pattern="^(taker_at_cross|maker)$")
    post_only: bool = False
    reduce_only: bool = False
    cancel_order_on_pause: bool = True
    expiration_time: Optional[int] = None  # unix seconds
    subaccount: int = 0
    order_group_id: Optional[str] = None


class CreateOrderResponseV2(BaseModel):
    """V2 response for order create (partial fill supported)."""

    order_id: str
    client_order_id: Optional[str] = None
    fill_count: str = "0.00"
    remaining_count: str = "0.00"
    average_fill_price: Optional[str] = None
    average_fee_paid: Optional[str] = None
    ts_ms: int
    simulated: bool = True


# Legacy stub kept for backward compat in Phase 1->2 transition (internal use only)
class OrderCreateRequest(BaseModel):
    """Legacy stub (yes/no). Deprecated; prefer CreateOrderRequestV2."""

    ticker: str
    side: str = Field(..., pattern="^(yes|no)$")
    action: str = Field(default="buy", pattern="^(buy|sell)$")
    price_dollars: str
    count: int = Field(..., ge=1)


class OrderResponse(BaseModel):
    """Legacy/minimal response."""

    order_id: str
    ticker: str
    side: str
    price_dollars: str
    count: int
    status: str = "open"
    simulated: bool = True


# --- API Key generation stub response ---

class GenerateApiKeyRequest(BaseModel):
    name: str = "paper-trading-key"
    scopes: list[str] = Field(default_factory=lambda: ["read", "write"])


class GenerateApiKeyResponse(BaseModel):
    """Returned once. Private key is PEM; never stored server-side after return."""

    api_key_id: str
    private_key: str  # full PEM — user must save immediately
    name: Optional[str] = None
    simulated: bool = True
    note: str = "This private key is shown only once. Store it securely. (Phase 1 stub)"


# --- Portfolio shapes (balance, positions, fills, orders) for authenticated /portfolio/* ---

class GetBalanceResponse(BaseModel):
    """Shape for GET /trade-api/v2/portfolio/balance (values in cents + dollars fp)."""

    balance: int  # cents available
    balance_dollars: str
    portfolio_value: int  # cents (marked value of positions)
    updated_ts: int
    simulated: bool = True


class MarketPosition(BaseModel):
    """Per-market position entry (simplified Kalshi fidelity)."""

    ticker: str
    position_fp: str  # signed, e.g. "5.00" long yes, "-3.00" short yes
    total_traded_dollars: str = "0.0000"
    market_exposure_dollars: str = "0.0000"
    realized_pnl_dollars: str = "0.0000"
    resting_orders_count: int = 0
    fees_paid_dollars: str = "0.0000"
    last_updated_ts: Optional[str] = None


class GetPositionsResponse(BaseModel):
    """Shape for GET /trade-api/v2/portfolio/positions"""

    market_positions: list[MarketPosition] = Field(default_factory=list)
    event_positions: list[dict] = Field(default_factory=list)  # stub for now
    cursor: str = ""


class FillResponse(BaseModel):
    """Single fill / trade execution."""

    fill_id: str
    order_id: str
    ticker: str
    count_fp: str
    yes_price_dollars: str
    no_price_dollars: str = "0.0000"  # derived
    is_taker: bool = True
    fee_cost: str = "0.0000"
    created_time: str


class GetFillsResponse(BaseModel):
    """Shape for GET /trade-api/v2/portfolio/fills"""

    fills: list[FillResponse]
    cursor: str = ""


class OrderSummary(BaseModel):
    """Open or recent order for /portfolio/orders list."""

    order_id: str
    client_order_id: Optional[str] = None
    ticker: str
    side: str
    price_dollars: str
    count: int
    filled_count: int = 0
    status: str  # open, filled, cancelled, ...
    created_at: Optional[str] = None


class GetOrdersResponse(BaseModel):
    """Response for listing user's orders."""

    orders: list[OrderSummary]
    cursor: str = ""
