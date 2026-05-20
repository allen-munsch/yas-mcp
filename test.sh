#!/bin/bash
set -e

# ── yas-mcp Test Runner ────────────────────────────────────────────────────
# Usage:
#   ./test.sh              # Run all tests (unit + adjuster)
#   ./test.sh unit         # Unit tests only
#   ./test.sh e2e          # End-to-end tests with docker compose
#   ./test.sh a2a          # A2A protocol e2e tests
#   ./test.sh auth         # Auth middleware + secrets e2e tests
#   ./test.sh all          # Everything (unit + e2e + a2a + auth)

MODE="${1:-unit}"

run_unit_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  UNIT TESTS"
    echo "══════════════════════════════════════════════"
    echo ""

    # Run lib tests (MCP processor, A2A, auth, secrets, parser, registry)
    echo "--- Lib Tests ---"
    cargo test --lib -- --nocapture
    echo ""

    # Run adjuster tests
    echo "--- Adjuster Tests ---"
    cargo test --test adjuster_tests -- --nocapture
    echo ""

    # Run stdio protocol tests (requires test-utils feature)
    echo "--- STDIO Protocol Tests ---"
    cargo test --test stdio_protocol_tests --features=test-utils -- --nocapture
    echo ""

    # Clippy check
    echo "--- Clippy ---"
    cargo clippy --all-targets --all-features -- -D warnings
    echo ""

    echo "✅ Unit tests complete"
}

run_e2e_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  END-TO-END TESTS (docker compose)"
    echo "══════════════════════════════════════════════"
    echo ""

    # Ensure the test config is set up
    export SWAGGER_FILE_PATH="${SWAGGER_FILE_PATH:-examples/todo-app/openapi.yaml}"
    export TEST_BEARER_TOKEN="${TEST_BEARER_TOKEN:-e2e-test-token-12345}"
    export SERVER_MODE="http"

    # Build and start services
    echo "Building services..."
    docker compose build yas-mcp prism e2e-tests

    echo "Starting services..."
    docker compose up -d yas-mcp prism

    echo "Waiting for services to be healthy..."
    sleep 5

    # Run e2e test container
    echo "Running e2e tests..."
    docker compose --profile e2e run --rm e2e-tests || true

    # Cleanup
    echo "Cleaning up..."
    docker compose down

    echo ""
    echo "✅ E2E tests complete"
}

run_a2a_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  A2A PROTOCOL TESTS"
    echo "══════════════════════════════════════════════"
    echo ""

    export SWAGGER_FILE_PATH="${SWAGGER_FILE_PATH:-examples/todo-app/openapi.yaml}"

    docker compose build yas-mcp a2a-tests
    docker compose up -d yas-mcp
    sleep 5
    docker compose --profile a2a run --rm a2a-tests || true
    docker compose down

    echo "✅ A2A tests complete"
}

run_auth_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  AUTH + SECRETS TESTS"
    echo "══════════════════════════════════════════════"
    echo ""

    export SWAGGER_FILE_PATH="${SWAGGER_FILE_PATH:-examples/todo-app/openapi.yaml}"
    export TEST_BEARER_TOKEN="${TEST_BEARER_TOKEN:-e2e-test-token-12345}"

    # Create a temp file secret for testing file:// resolver
    mkdir -p /tmp/yas-mcp-secrets
    echo "e2e-test-token-12345" > /tmp/yas-mcp-secrets/test-bearer-token

    docker compose build yas-mcp auth-tests
    docker compose up -d yas-mcp
    sleep 5
    docker compose --profile auth-test run --rm auth-tests || true
    docker compose down

    rm -rf /tmp/yas-mcp-secrets
    echo "✅ Auth tests complete"
}

run_probe() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  FLYING PROBE — System Board Test"
    echo "══════════════════════════════════════════════"
    echo ""
    bash scripts/flying-probe.sh "$@"
}

run_full_e2e_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  FULL-STACK E2E TESTS (Dex OIDC + A2A + MCP)"
    echo "══════════════════════════════════════════════"
    echo ""

    export SWAGGER_FILE_PATH="${SWAGGER_FILE_PATH:-examples/todo-app/openapi.yaml}"
    export E2E_OIDC_CLIENT_SECRET="e2e-client-secret"

    # Build everything
    docker compose build yas-mcp prism dex e2e-full-tests

    # Start services with oidc profile (includes dex)
    docker compose --profile oidc up -d yas-mcp prism dex

    echo "Waiting for services to be healthy..."
    sleep 8

    # Run the full-stack e2e test
    docker compose --profile e2e-full run --rm e2e-full-tests || true

    # Cleanup
    docker compose --profile oidc --profile e2e-full down

    echo "✅ Full-stack E2E tests complete"
}

run_docker_unit_tests() {
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  UNIT TESTS (in Docker)"
    echo "══════════════════════════════════════════════"
    echo ""

    docker compose build unit-tests
    docker compose --profile unit-test run --rm unit-tests
    docker compose down

    echo "✅ Docker unit tests complete"
}

case "$MODE" in
    unit)
        run_unit_tests
        ;;
    e2e)
        run_e2e_tests
        ;;
    a2a)
        run_a2a_tests
        ;;
    auth)
        run_auth_tests
        ;;
    docker)
        run_docker_unit_tests
        ;;
    probe)
        run_probe "$@"
        ;;
    full)
        run_full_e2e_tests
        ;;
    all)
        run_unit_tests
        run_e2e_tests
        run_a2a_tests
        run_auth_tests
        run_full_e2e_tests
        ;;
    *)
        echo "Usage: $0 {unit|e2e|a2a|auth|docker|full|all}"
        echo ""
        echo "  unit    - Run unit tests + clippy (no server needed)"
        echo "  e2e     - Run end-to-end tests with docker compose"
        echo "  a2a     - Run A2A protocol e2e tests"
        echo "  auth    - Run auth middleware + secrets e2e tests"
        echo "  docker  - Run unit tests inside Docker"
        echo "  probe   - Flying probe: systematically tests every endpoint"
        echo "  full    - Full-stack E2E: OpenAPI → MCP → A2A → Dex OIDC → auth"
        echo "  all     - Run everything"
        exit 1
        ;;
esac
