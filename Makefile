.PHONY: help up down logs clean test test-sims test-admin lint admin install-admin

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

test: test-sims test-admin ## Run every test suite (sim + admin)

test-sims: ## Run the Python simulator test suite
	cd polymarket-sim && python -m pytest -q

test-admin: ## Run the Rust admin test suite
	cd ratatui-admin && cargo test

lint: ## Run linters across the repo
	cd polymarket-sim && ruff check src/ tests/
	cd ratatui-admin && cargo clippy --quiet

clean: ## Remove containers, volumes, and caches
	docker compose down -v
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".ruff_cache" -exec rm -rf {} + 2>/dev/null || true
	cd ratatui-admin && cargo clean 2>/dev/null || true
