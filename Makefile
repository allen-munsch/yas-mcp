.PHONY: build run test clean docker-build docker-run docker-compose-up docker-compose-down
.PHONY: test-unit test-e2e test-a2a test-auth test-full test-all probe demo

# Build the project
build:
	cargo build --release

# Run the project
run:
	cargo run -- --swagger-file examples/petstore.yaml

# Run unit tests
test: test-unit

# Unit tests (no server needed)
test-unit:
	cargo test --lib
	cargo test --test adjuster_tests
	cargo test --test stdio_protocol_tests --features=test-utils

# End-to-end tests with docker compose
test-e2e:
	bash test.sh e2e

# A2A protocol e2e tests
test-a2a:
	bash test.sh a2a

# Auth middleware + secrets e2e tests
test-auth:
	bash test.sh auth

# Flying probe — comprehensive system verification
probe:
	@MCP_SERVER_URL=http://127.0.0.1:${OPENAPI_MCP_PORT:-3002} bash scripts/flying-probe.sh

# Demo — starts everything + runs the probe
demo:
	@echo ""
	@echo "  ╔══════════════════════════════════════════════════════╗"
	@echo "  ║  ☀️  yas-mcp DEMO — Let's get you running!           ║"
	@echo "  ╚══════════════════════════════════════════════════════╝"
	@echo ""
	@echo "  Starting services..."
	@docker compose up -d
	@sleep 3
	@echo ""
	@echo "  Checking health..."
	@curl -sf http://localhost:3002/health > /dev/null && echo "  ✅ Server is healthy!" || echo "  ⚠️  Server may still be starting..."
	@echo ""
	@echo "  Available endpoints:"
	@echo "    Health:   http://localhost:3002/health"
	@echo "    MCP:      http://localhost:3002/mcp"
	@echo "    Metrics:  http://localhost:3002/metrics"
	@echo "    Catalog:  http://localhost:3002/.well-known/ai-catalog.json"
	@echo ""
	@echo "  Quick test — listing tools:"
	@curl -s -X POST http://localhost:3002/mcp -H "Content-Type: application/json" \
	  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | \
	  jq -r '.result.tools | length | "  ✅ \(.) tools available"' 2>/dev/null || echo "  (server starting up...)"
	@echo ""
	@echo "  🧪 Running flying probe..."
	@MCP_SERVER_URL=http://127.0.0.1:3002 bash scripts/flying-probe.sh --quick || true
	@echo ""
	@echo "  🎉 Demo ready! Try:"
	@echo "     curl http://localhost:3002/metrics"
	@echo "     curl http://localhost:3002/.well-known/agent-card.json | jq"
	@echo "     make probe     # full system test"
	@echo "     docker compose down  # when you're done"
	@echo ""

# Full-stack E2E (OpenAPI → MCP → A2A → Dex OIDC → auth)
test-full:
	bash test.sh full

# Run everything (unit + e2e + a2a + auth + full)
test-all:
	bash test.sh all

# Clean build artifacts
clean:
	cargo clean

# Docker builds
docker-build:
	docker build -t yas-mcp:latest .

docker-build-prod:
	docker build -f Dockerfile.prod -t yas-mcp:prod .

# Docker run
docker-run:
	docker run -p 8080:8080 \
		-v $(PWD)/examples/petstore.yaml:/app/config/swagger.json \
		-v $(PWD)/adjustments.yaml:/app/config/adjustments.yaml \
		yas-mcp:latest

# Docker Compose
docker-compose-up:
	docker compose up -d

docker-compose-down:
	docker compose down

docker-compose-logs:
	docker compose logs -f

# Development with hot reload (requires cargo-watch)
dev:
	cargo watch -x 'run -- --swagger-file examples/petstore.yaml'

# Format code
fmt:
	cargo fmt

# Lint code
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Build for release
release: test-unit fmt lint build

# Build static binaries for distribution (linux/amd64 + linux/arm64)
release-linux:
	cargo build --release --target x86_64-unknown-linux-musl
	cargo build --release --target aarch64-unknown-linux-musl
	@echo ""
	@echo "  Binaries:"
	@ls -lh target/x86_64-unknown-linux-musl/release/yas-mcp
	@ls -lh target/aarch64-unknown-linux-musl/release/yas-mcp
	@echo ""
	@echo "  Copy to releases:"
	@echo "  cp target/x86_64-unknown-linux-musl/release/yas-mcp sdks/node/bin/yas-mcp"
	@echo "  cp target/aarch64-unknown-linux-musl/release/yas-mcp sdks/node/bin/yas-mcp-arm64"

# Quick install for local dev (builds and copies to /usr/local/bin)
install-local:
	cargo build --release
	sudo cp target/release/yas-mcp /usr/local/bin/yas-mcp
	@echo "✅ yas-mcp installed to /usr/local/bin/yas-mcp"
