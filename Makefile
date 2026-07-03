.PHONY: help up down logs clean test test-sims test-admin test-gui test-leaderboard lint admin install-admin gui install-gui

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

up: ## Start the Polymarket simulator + leaderboard with Docker Compose
	docker compose up -d --build

down: ## Stop all services
	docker compose down

logs: ## Follow logs from the simulator and services
	docker compose logs -f

admin: ## Run the Rust admin TUI (against running sims)
	cd ratatui-admin && cargo run --release

install-admin: ## Install the `predlab` admin binary onto your PATH
	cd ratatui-admin && cargo install --path .

gui: ## Run the desktop GUI (against the configured servers)
	cargo run -p predlab-gui --release

install-gui: ## Install the `predlab-gui` desktop binary onto your PATH
	cargo install --path predlab-gui --locked

test: test-sims test-admin test-gui test-leaderboard ## Run every test suite

test-sims: ## Run the Python simulator test suite
	cd polymarket-sim && python -m pytest -q

test-admin: ## Run the Rust admin test suite
	cd ratatui-admin && cargo test

test-gui: ## Run the desktop GUI test suite
	cd predlab-gui && cargo test

test-leaderboard: ## Run the leaderboard web-server test suite
	cd leaderboard-rs && cargo test

lint: ## Run linters across the repo
	cd polymarket-sim && ruff check src/ tests/
	cd ratatui-admin && cargo clippy --quiet
	cd predlab-gui && cargo clippy --quiet
	cd leaderboard-rs && cargo clippy --quiet

clean: ## Remove containers, volumes, and caches
	docker compose down -v
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".ruff_cache" -exec rm -rf {} + 2>/dev/null || true
	cd ratatui-admin && cargo clean 2>/dev/null || true
	cd predlab-gui && cargo clean 2>/dev/null || true
	cd leaderboard-rs && cargo clean 2>/dev/null || true
