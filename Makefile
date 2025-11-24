.PHONY: build run test clean docker-build docker-run docker-compose-up docker-compose-down

# Build the project
build:
	cargo build --release

# Run the project
run:
	cargo run -- --swagger-file examples/petstore.yaml

# Run tests
test:
	cargo test

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
	cargo clippy

# Build for release
release: test fmt lint build
