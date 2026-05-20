"""
Kalshi Trade API v2 Simulator — FastAPI entrypoint (Phase 1 Foundation).

App factory + lifespan pattern.
Exposes the real Kalshi paths under /trade-api/v2 so that the official
Python/TS SDKs can be pointed at this server with base_url override.

Run:
    uvicorn kalshi_sim.main:app --port 8002 --reload
or
    python -m kalshi_sim.main
"""
import logging
from contextlib import asynccontextmanager
from typing import Any, AsyncIterator

from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from .api.auth import router as auth_router
from .api.markets import router as markets_router
from .api.orderbooks import router as orderbooks_router
from .api.orders import router as orders_router
from .clients.kalshi_client import get_kalshi_client
from .config import ensure_data_dir, get_settings
from .db import init_db
from .services.orderbook import rebuild_books_from_db
from .services.sync import sync_markets

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s | %(levelname)s | %(name)s | %(message)s",
)
logger = logging.getLogger("kalshi_sim")


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Startup / shutdown lifecycle."""
    settings = get_settings()
    logger.info(f"Starting kalshi-sim in {settings.environment} mode on port {settings.port}")

    ensure_data_dir()
    init_db()

    # Prime the cache with a few live markets so first curl is interesting
    try:
        client = await get_kalshi_client()
        # Use a sync session context for the initial load

        from .db import db_session
        with db_session() as db:
            await sync_markets(db, client, limit=30, status="open")
            loaded = rebuild_books_from_db(db)
            logger.info(f"Initial market sync complete — live Kalshi data ready ({loaded} open orders restored to books)")
    except Exception as exc:
        logger.warning(f"Initial sync failed (will lazy-sync on first request): {exc}")

    yield

    # shutdown
    client = await get_kalshi_client()
    await client.close()
    logger.info("kalshi-sim shutdown complete")


def create_app() -> FastAPI:
    """Factory — returns a fully configured FastAPI app."""
    settings = get_settings()

    app = FastAPI(
        title="Kalshi Trade API Simulator",
        description=(
            "Educational paper-trading clone of Kalshi's Trade API v2. "
            "Live prices from real Kalshi, all trading is paper only. "
            "Point your existing Kalshi SDKs here for strategy testing."
        ),
        version="0.1.0",
        lifespan=lifespan,
        docs_url="/docs",
        redoc_url="/redoc",
    )

    # Allow the terminal website to call the APIs directly from the browser
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Health (simple, fast)
    @app.get("/health", tags=["system"])
    async def health() -> dict[str, Any]:
        return {
            "status": "ok",
            "service": "kalshi-sim",
            "version": "0.1.0",
            "env": settings.environment,
        }

    # Root info
    @app.get("/", tags=["system"])
    async def root() -> dict[str, Any]:
        return {
            "message": "Kalshi Trade API v2 Simulator (paper trading)",
            "docs": "/docs",
            "health": "/health",
            "markets_example": "/trade-api/v2/markets?limit=5&status=open",
            "warning": "PAPER TRADING ONLY — NOT AFFILIATED WITH KALSHI",
        }

    # Mount Kalshi-shaped routers (exact paths)
    app.include_router(markets_router)
    app.include_router(orderbooks_router)
    app.include_router(orders_router)
    app.include_router(auth_router)

    # Global exception handler (clean JSON)
    @app.exception_handler(Exception)
    async def unhandled_exc(request: Request, exc: Exception) -> JSONResponse:
        logger.exception("Unhandled error")
        return JSONResponse(
            status_code=500,
            content={"detail": "Internal error", "type": type(exc).__name__},
        )

    return app


app = create_app()


def main() -> None:
    """Entry point for `python -m kalshi_sim.main` or `kalshi-sim` console script."""
    import uvicorn

    settings = get_settings()
    logger.info(f"Running uvicorn on 0.0.0.0:{settings.port}")
    uvicorn.run(
        "kalshi_sim.main:app",
        host="0.0.0.0",
        port=settings.port,
        reload=settings.environment == "development",
        log_level=settings.log_level.lower(),
    )


if __name__ == "__main__":
    main()
