.PHONY: help up down logs clean test test-sims test-gui test-leaderboard lint gui install-gui

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

up: ## Start the Polymarket simulator + leaderboard with Docker Compose
	docker compose up -d --build

down: ## Stop all services
	docker compose down

logs: ## Follow logs from the simulator and services
	docker compose logs -f

gui: ## Run the desktop GUI (against the configured servers)
	cargo run -p predlab-gui --release

install-gui: ## Install the `predlab-gui` desktop binary onto your PATH
	cargo install --path predlab-gui --locked

test: test-sims test-gui test-leaderboard ## Run every test suite

test-sims: ## Run the Python simulator test suite
	cd polymarket-sim && python -m pytest -q

test-gui: ## Run the workspace (desktop GUI + util) test suite
	cargo test --workspace

test-leaderboard: ## Run the leaderboard web-server test suite
	cd leaderboard-rs && cargo test

lint: ## Run linters across the repo
	cd polymarket-sim && ruff check src/ tests/
	cargo clippy --workspace --quiet
	cd leaderboard-rs && cargo clippy --quiet

clean: ## Remove containers, volumes, and caches
	docker compose down -v
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".ruff_cache" -exec rm -rf {} + 2>/dev/null || true
	cargo clean 2>/dev/null || true
	cd leaderboard-rs && cargo clean 2>/dev/null || true
