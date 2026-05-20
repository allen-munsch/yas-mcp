# Contributing to yas-mcp

Thanks for wanting to help! Here's how to get started.

## Quick Setup

```bash
git clone https://github.com/allen-munsch/yas-mcp.git
cd yas-mcp
cargo build
cargo test
```

## Project Structure

```
yas-mcp/
├── src/
│   ├── main.rs              # Entry point
│   ├── lib.rs               # Public API re-exports
│   ├── cli.rs               # CLI argument parsing
│   └── internal/
│       ├── a2a/             # A2A protocol (agent card, task store, SSE)
│       ├── auth/            # OIDC discovery, JWKS, OAuth2 providers
│       ├── catalog/         # AI Catalog auto-generation
│       ├── config/          # Layered config (YAML + env + CLI)
│       ├── control/         # Rate limiting, circuit breakers, caching
│       ├── mcp/             # MCP processor, tool registry, protocol
│       ├── parser/          # OpenAPI 3.x parser
│       ├── requester/       # HTTP client, route executors
│       ├── secrets/         # Secret resolution (env://, file://)
│       ├── server/          # HTTP server, tool handler, middleware
│       ├── telemetry/       # Prometheus metrics
│       └── transport/       # STDIO, mock transport, runner
├── tests/                   # Integration + adjuster tests
├── benches/                 # Criterion benchmarks
├── deploy/minikube/         # Kubernetes Kustomize manifests
├── scripts/                 # Test scripts, flying probe
├── configs/                 # Example configs (e2e, dex, oauth)
├── examples/                # OpenAPI specs for testing
└── docs/                    # Architecture, phase plan, guides
```

## Before Submitting

```bash
make fmt          # cargo fmt
make lint         # cargo clippy --all-targets
make test-unit    # 200+ unit tests
make test-all     # everything (unit + e2e + a2a + full stack)
```

## Code Conventions

- **Tests alongside code**: Unit tests go in `#[cfg(test)] mod tests` at the bottom of each source file
- **Traits for testability**: Use traits (`Parser`, `AuthProvider`, `SecretResolver`) to enable mock implementations
- **Config over code**: Everything configurable via YAML, env vars, or CLI — no hardcoded values
- **Async-first**: All I/O is async via tokio. Use `std::sync::Mutex` for short-lived locks, `tokio::sync::Mutex` for locks held across await points
- **Error handling**: `anyhow::Result` with `.context()` for rich error chains. `thiserror` for library error types

## Adding a New Feature

1. Find the right module in `src/internal/`
2. Add your code with unit tests
3. If it's a new capability, add config in `src/internal/config/config.rs`
4. Wire it into `src/internal/server/server.rs` if it's a runtime feature
5. Add integration test in `tests/`
6. Update docs in `docs/`
7. Run `make test-all` to verify nothing is broken

## Adding a New Auth Provider

Implement the `AuthProvider` trait in `src/internal/auth/provider.rs`:

```rust
struct MyProvider { ... }

impl AuthProvider for MyProvider {
    fn provider_type(&self) -> &str { "my_provider" }
    fn authenticate(&self, headers: &HashMap<String, String>) -> Result<Option<AuthIdentity>> { ... }
    fn matches_route(&self, path: &str) -> bool { ... }
}
```

Register it in `build_auth_middleware()` in `src/internal/server/server.rs`.

## Adding a New Secret Backend

Implement the `SecretResolver` trait in `src/internal/secrets/resolver.rs`:

```rust
struct VaultResolver { ... }

#[async_trait]
impl SecretResolver for VaultResolver {
    fn scheme(&self) -> &str { "vault" }
    async fn resolve(&self, secret_ref: &SecretRef) -> Result<String> { ... }
}
```

Register it in `build_secret_store()` in `src/internal/server/server.rs`.

## Flying Probe

The flying probe (`scripts/flying-probe.sh`) systematically tests every endpoint. Run it against a local server before submitting:

```bash
cargo run -- --swagger-file examples/todo-app/openapi.yaml --mode http &
MCP_SERVER_URL=http://127.0.0.1:3000 bash scripts/flying-probe.sh
```

## Questions?

Open an issue or look at the existing code — most patterns have examples in the codebase already.
