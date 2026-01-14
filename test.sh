#!/bin/bash
set -e

# Run unit tests first (no server needed)
echo "Running unit tests..."
cargo test --test stdio_protocol_tests --features="test-utils"
cargo test --test adjuster_tests

# Start containers for integration tests
echo "Starting containers..."
export SWAGGER_FILE_PATH=examples/todo-app/openapi.yaml

# docker compose build yas-mcp -d
docker compose up yas-mcp prism -d

# Wait for services to be healthy
sleep 15
# Run integration tests
echo "Running integration tests..."
cargo test --test integration_tests -- --nocapture
cargo test --test debug_tests -- --nocapture
cargo test --test debug_requester_configuration -- --nocapture

# Cleanup
echo "Cleaning up..."
docker compose down

echo "All tests completed!"