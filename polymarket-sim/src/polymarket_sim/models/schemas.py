"""
Pydantic schemas (request/response models) for the Polymarket simulator API.

Real Polymarket shapes are approximated for /markets so existing clients see familiar data.
"""

from datetime import datetime

from pydantic import BaseModel, Field


class HealthResponse(BaseModel):
    status: str = "ok"
    version: str
    environment: str


class MarketOut(BaseModel):
    """Shape returned by GET /markets — close to real Gamma response for fidelity."""

    id: str
    conditionId: str = Field(alias="conditionId")
    question: str
    slug: str
    outcomes: list[str]
    outcomePrices: list[str] = Field(alias="outcomePrices")
    clobTokenIds: list[str] | None = Field(default=None, alias="clobTokenIds")
    bestBid: float | None = Field(default=None, alias="bestBid")
    bestAsk: float | None = Field(default=None, alias="bestAsk")
    lastTradePrice: float | None = Field(default=None, alias="lastTradePrice")
    volume: float | None = None
    liquidity: float | None = None
    active: bool
    closed: bool
    updatedAt: datetime | None = Field(default=None, alias="updatedAt")

    model_config = {"populate_by_name": True, "from_attributes": True}


# CLOB-compatible response shapes (for py-clob-client / SDK drop-in)
class MidpointOut(BaseModel):
    midpoint: str


class SpreadOut(BaseModel):
    spread: str


class LastTradePriceOut(BaseModel):
    lastTradePrice: str


class PostOrderResponse(BaseModel):
    """Minimal shape expected by many clients after POST /order or /orders."""

    success: bool = True
    orderID: str
    status: str = "open"
    errorMsg: str | None = None


class UserOrderOut(BaseModel):
    id: int
    market_id: str
    clob_token_id: str | None
    side: str
    price: float | None
    size: float
    filled_size: float
    status: str
    created_at: datetime


class PositionWithPnLOut(BaseModel):
    market_id: str
    clob_token_id: str
    size: float
    avg_entry_price: float | None
    current_price: float
    unrealized_pnl: float
    market_question: str | None = None


class PortfolioOut(BaseModel):
    """Authenticated account summary: free cash, marked position value, net worth."""

    cash: float
    positions_value: float
    open_orders_value: float = 0.0  # cash escrowed in resting buy orders
    net_worth: float
