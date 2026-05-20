# ☀️ yas-mcp &middot; [![ci](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml) [![license](https://img.shields.io/badge/license-AGPL--3.0--only-red)](LICENSE)

**Turn any REST API into AI tools — in 30 seconds.**

Point yas-mcp at an OpenAPI spec and it becomes an MCP server your AI assistant can call. It also speaks the A2A protocol for agent-to-agent communication. Auth, metrics, and caching are built in.

```bash
# 1. Point at an OpenAPI spec
yas-mcp --swagger-file https://petstore3.swagger.io/api/v3/openapi.json --mode http

# 2. That's it. Your AI can now call:
#    • tools/list  → "What can you do?"
#    • tools/call  → "List all pets, create an order, update user..."
```

## ✨ What It Does

| You have | yas-mcp gives you |
|----------|-------------------|
| An OpenAPI/Swagger file | An MCP server with typed tools |
| An OIDC provider URL | Auto-discovered OAuth2 protection |
| Multiple APIs | One proxy serving all of them |
| AI agents that need to chat | A2A protocol for agent-to-agent delegation |
| Production requirements | Metrics, rate limiting, circuit breakers, caching |

```
   OpenAPI Spec                yas-mcp                   AI Assistant
  ┌──────────────┐         ┌──────────────┐         ┌──────────────┐
  │  GET /pets   │────────▶│  listPets()  │────────▶│  "List all    │
  │  POST /pets  │         │  createPet() │         │   available  │
  │  GET /store  │         │  getOrders() │         │   pets"      │
  │  ...         │         │  ...         │         └──────────────┘
  └──────────────┘         └──────┬───────┘
                                  │
                        ┌─────────┼─────────┐
                        │         │         │
                  ┌─────▼────┐ ┌─▼──────┐ ┌─▼──────────┐
                  │  Dex     │ │  A2A   │ │ Prometheus │
                  │  (OIDC)  │ │ Agents │ │  /metrics  │
                  └──────────┘ └────────┘ └────────────┘
```

## 🚀 Quickstart

### Option 1: Docker Compose (recommended — everything included)

```bash
git clone https://github.com/allen-munsch/yas-mcp.git
cd yas-mcp

# Start with the built-in Todo API example
docker compose up -d

# Verify it's alive
curl http://localhost:3000/health
# → OK

# List available tools
curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | jq

# Run the flying probe to verify everything
make probe
```

### Option 2: Build from source

```bash
cargo build --release
./target/release/yas-mcp \
  --swagger-file examples/petstore.yaml \
  --mode http \
  --port 3000
```

### Option 3: Use your own API

```bash
docker compose up -d
# Edit the mounted config to point at your API
# Or:
export SWAGGER_FILE_PATH=~/my-api/openapi.yaml
docker compose up -d
```

## 🧪 Flying Probe

Like a circuit board tester, the flying probe systematically verifies every surface of your yas-mcp deployment:

```bash
# Local probe
bash scripts/flying-probe.sh

# Docker probe
docker compose --profile probe run --rm flying-probe

# Kubernetes — runs continuously as a sidecar + CronJob
kubectl apply -k deploy/minikube/base
```

**7 boards tested:** Health & Discovery → MCP Protocol → Tool Calls → A2A Lifecycle → Auth Middleware → Rate Limits → Signal Quality. See [scripts/flying-probe.sh](scripts/flying-probe.sh).

## 📋 Configuration

Everything goes in `config.yaml`. Here's the full menu:

```yaml
server:
  mode: http          # stdio | http
  port: 3000
  host: 0.0.0.0

# Give it an OpenAPI spec — yas-mcp does the rest
swagger_file: "examples/todo-app/openapi.yaml"

# Auto-discover OIDC — just paste the issuer URL
oauth:
  enabled: true
  provider: oidc
  issuer_url: "https://dex.example.com"
  client_id: "${OIDC_CLIENT_ID}"
  client_secret: "env://OIDC_CLIENT_SECRET"  # ← secret refs!

# A2A protocol for agent-to-agent delegation
a2a:
  enabled: true
  agent_card_name: "My API Agent"

# Production safeguards
cache:
  enabled: true
  default_ttl_secs: 60

# Auth middleware — chain multiple providers
auth:
  middleware_chain:
    - type: bearer_token
      route_filter: "/api/**"
      config:
        token: "env://API_TOKEN"
```

### Secret References

Never put raw secrets in config. Use references instead:

| Reference | Source |
|-----------|--------|
| `env://MY_VAR` | Environment variable |
| `file:///run/secrets/token` | Docker/K8s secret file |
| `literal://value` | Explicit literal (for clarity) |

## 🏗️ Architecture

```
src/
├── internal/
│   ├── a2a/           A2A protocol (agent card, task store, SSE streaming)
│   ├── auth/          OIDC discovery, JWKS validation, OAuth2 providers
│   ├── catalog/       AI Catalog auto-generation
│   ├── config/        Layered config (YAML + env + CLI)
│   ├── control/       Rate limiting, circuit breakers, response caching
│   ├── mcp/           MCP processor, tool registry, protocol types
│   ├── parser/        OpenAPI 3.x parser (YAML/JSON)
│   ├── requester/     HTTP client, route executors
│   ├── secrets/       Secret resolution (env://, file://, custom backends)
│   ├── server/        HTTP server, tool handler, auth middleware
│   ├── telemetry/     Prometheus metrics
│   └── transport/     STDIO transport, mock transport, runner
```

## 🔧 Development

```bash
make build          # cargo build --release
make test-unit      # 214 unit tests
make lint           # clippy --all-targets
make fmt            # cargo fmt

make probe          # flying probe against local server
make test-e2e       # docker compose integration tests
make test-a2a       # A2A protocol tests
make test-full      # full stack: Dex OIDC + MCP + A2A
make test-all       # everything
```

## 📊 Endpoints

| Endpoint | Method | What |
|----------|--------|------|
| `/health` | GET | Health check |
| `/metrics` | GET | Prometheus metrics (counters, histograms, gauges) |
| `/mcp` | POST | MCP JSON-RPC (tools/list, tools/call, initialize, ping) |
| `/.well-known/agent-card.json` | GET | A2A Agent Card (discovery) |
| `/a2a/tasks/send` | POST | Submit A2A task |
| `/a2a/tasks/sendSubscribe` | POST | Submit + SSE streaming updates |
| `/a2a/tasks/get` | GET | Get task status |
| `/a2a/tasks/cancel` | POST | Cancel a task |
| `/.well-known/ai-catalog.json` | GET | AI Catalog (cross-protocol discovery) |

## 🛡️ Production Features

- ✅ Prometheus metrics with per-tool counters, histograms, error rates
- ✅ Rate limiting — token bucket per client, configurable burst/refill
- ✅ Circuit breakers — per-upstream failure detection, half-open probing
- ✅ Response caching — TTL-based, per-route overrides, invalidation
- ✅ Graceful shutdown — SIGTERM drain with in-flight request completion
- ✅ Auth middleware chain — bearer token, API key, custom providers
- ✅ OIDC discovery — `issuer_url` → auto-configured auth endpoints
- ✅ JWKS validation — JWT signature verification with key rotation
- ✅ Secret references — `env://`, `file://`, never hardcode secrets
- ✅ Flying probe — continuous system health verification

## 📚 More Docs

- [Quickstart Guide](docs/QUICKSTART.md) — 60-second setup
- [Architecture](docs/ARCHITECTURE.md) — Component map and data flow
- [A2A Integration](docs/A2A_INTEGRATION.md) — Agent-to-Agent protocol
- [OIDC Setup](docs/OIDC_PLUGIN.md) — OIDC discovery and multi-tenant auth
- [Deployment](docs/DEPLOY_MINIKUBE.md) — Kubernetes on Minikube
- [Phase Plan](docs/PHASE_PLAN.md) — Roadmap and priorities

[GNU Affero General Public License v3.0 only](LICENSE)


## License

[AGPL-3.0-only](LICENSE)
