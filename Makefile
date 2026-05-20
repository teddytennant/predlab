.PHONY: help install dev up down tui logs clean lint

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-15s\033[0m %s\n", $$1, $$2}'

install: ## Install the PredLab TUI in editable mode
	pip install -e ".[dev]"

dev: install up ## Install + start both simulators

up: ## Start both simulators with Docker Compose
	docker compose up -d --build

down: ## Stop all services
	docker compose down

tui: ## Run the PredLab admin TUI
	predlab

logs: ## Follow logs from both simulators
	docker compose logs -f

clean: ## Remove containers, volumes, and Python cache
	docker compose down -v
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name ".ruff_cache" -exec rm -rf {} + 2>/dev/null || true

lint: ## Run linters
	ruff check src/
	ruff format --check src/
