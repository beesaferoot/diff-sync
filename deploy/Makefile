# Shortcuts for diff-sync deployment. Run from the repo root: `make -C deploy <target>`
COMPOSE = docker compose -f docker-compose.yml --env-file .env

# Local dev: build the image for THIS machine and run app-only (no env file needed).
dev: ; docker compose -f docker-compose.yml up -d --build

# VPS: pull the prebuilt GHCR image and (re)start. No build on the box.
deploy: ; $(COMPOSE) pull && $(COMPOSE) up -d && docker image prune -f

logs: ; $(COMPOSE) logs -f app

down: ; $(COMPOSE) down

.PHONY: dev deploy logs down
