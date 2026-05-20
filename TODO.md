# yas-mcp Roadmap

> ✅ = done · 🚧 = in progress · 📋 = planned

## ✅ Completed

- [x] OpenAPI 3.0 → MCP tool generation
- [x] Swagger 2.0 → OpenAPI 3.0 normalization
- [x] OpenAPI 3.1 → OpenAPI 3.0 downgrade (native 3.1 planned)
- [x] HTTP + STDIO transport modes
- [x] A2A protocol (agent card, task store, SSE streaming, 5 endpoints)
- [x] OIDC Discovery (auto `.well-known/openid-configuration` fetch)
- [x] JWKS validation (JWT signature verification, key rotation)
- [x] OAuth2 providers (GitHub, Google, Microsoft, generic, OIDC)
- [x] Auth middleware (bearer token, API key, route-based provider chain)
- [x] Secret references (`env://`, `file://`, extensible `SecretResolver` trait)
- [x] Prometheus `/metrics` (counters, histograms, gauges per tool)
- [x] Rate limiting (token bucket per client)
- [x] Circuit breakers (per-upstream, half-open probing)
- [x] Response caching (TTL-based, per-route overrides)
- [x] AI Catalog (`/.well-known/ai-catalog.json`)
- [x] Graceful shutdown (SIGTERM/SIGINT with connection draining)
- [x] Flying probe (7-board comprehensive system verification)
- [x] Kubernetes deployment (Kustomize base + sidecar probe + CronJob)
- [x] Docker Compose (`docker compose up -d` + `make demo`)
- [x] Tracing spans (`mcp.tool_call`, `a2a.task_send`)
- [x] Mock mode — generate responses from OpenAPI schemas, no upstream needed
- [x] 246 unit tests (with features) + 27 adjuster + 10 integration + 7 stdio = 290 total
- [x] 0 clippy warnings

## 📋 Next

### Distribution (all via GitHub)

- [x] **Install script** — `curl ... | sh` from raw GitHub URL
- [x] **Docker image** — `docker compose up -d` from repo
- [x] **Thin wrappers** — JS/Python helpers in `sdks/` (copy what you need, zero deps)
- [ ] **Release CI** — GitHub Actions builds linux/amd64, linux/arm64, macOS binaries
- [ ] **Pre-built binaries** — attach to GitHub Releases for the install script to fetch
- [ ] **crates.io** — `cargo install yas-mcp`

### Sandbox Support

- [x] **bwrap** — bubblewrap sandbox config at `sandboxes/bwrap/run.sh`
- [x] **Docker** — Dockerfile + compose with healthcheck and resource limits
- [x] **Kubernetes** — Kustomize base with deployment, service, sidecar probe, CronJob
- [x] **seatbelt** — macOS sandbox profile at `sandboxes/seatbelt/yas-mcp.sb`
- [ ] **libkrun** — Firecracker microVM config
- [ ] **Zypi** — native Zypi sandbox integration (Firecracker ephemeral VM per tool call)
- [ ] **Sandbox proxy** — instead of running tools locally, proxy execution to an
  external sandbox service. Each tool call gets forwarded to a Firecracker VM,
  container, or edge function. yas-mcp becomes a secure gateway that never
  touches the upstream API directly — the sandbox service handles all I/O.
  ```
  AI client → yas-mcp → sandbox service → upstream API
                          (Firecracker/container/lambda)
  ```

### Protocol & Spec Support

- [ ] **Native OpenAPI 3.1** — parse JSON Schema directly, bypass `openapiv3` crate
- [ ] **Multi-file `$ref` resolution** — follow `$ref` across spec files
- [ ] **gRPC transport** — alternative to HTTP for high-throughput internal use
  - Google is pushing gRPC as the future MCP transport (lower latency, binary encoding,
    HTTP/2 multiplexing, native streaming). When the MCP community standardizes the
    protobuf schema, yas-mcp should support `--mode grpc` alongside http/stdio.
  - Auto-generate protobuf service definitions from the OpenAPI spec.
  - Use `tonic` (Rust gRPC framework) — already in the ecosystem.
- [ ] **WebSocket streaming** — for real-time tool responses

### Auth & Security

- [ ] **mTLS support** — client certificate authentication
- [ ] **OAuth2 device flow** — for CLI-based auth
- [ ] **Token introspection** — validate opaque tokens via introspection endpoint
- [ ] **Audit logging** — structured JSON audit trail per request

### Observability

- [ ] **OpenTelemetry OTLP export** — send spans to collector
- [ ] **Per-client/IP metrics** — request count per caller
- [ ] **Structured JSON logging** — machine-parseable log format
- [ ] **Grafana dashboard** — pre-built dashboard JSON

### Tool Composition & Response Shaping

- [ ] **Tool composition grammar** — chain multiple API calls into one MCP tool
  ```yaml
  compositions:
    - name: getCustomerWithOrders
      steps:
        - tool: get_customer
          params: { customer_id: "$input.id" }
          output: customer
        - tool: get_orders
          params: { customer_id: "$steps.customer.id" }
          output: orders
      output:
        customer: "$steps.customer"
        orders: "$steps.orders"
  ```
- [ ] **Response transforms** — filter API responses to reduce token waste
  ```yaml
  transforms:
    - path: /users
      methods: [GET]
      output:
        fields: [id, name, email]     # keep only these
    - path: /search
      methods: [GET]
      output:
        max_items: 5                   # truncate arrays
  ```

### Delightful UX

- [ ] **`yas-mcp init`** — interactive config wizard. Asks "where's your API?",
  detects auth, writes config.yaml. No docs needed.
- [ ] **`yas-mcp --dry-run`** — parse spec, list tools, validate config, exit.
  No server started. CI-ready: `yas-mcp --dry-run --swagger-file api.yaml`
- [ ] **`yas-mcp call <tool> --param value`** — call tools directly from CLI.
  `yas-mcp call get_users --page 1`. Debug without an MCP client.
- [ ] **`yas-mcp --demo`** — zero-config, boots with built-in petstore, prints URL.
- [ ] **`yas-mcp tools search <query>`** — find tools by name or description.
- [ ] **Auto-detect spec** — `yas-mcp --swagger-file .` finds local specs.
- [ ] **Pretty output** — `--format table` for humans, `--format json` for pipes.

### Mock & Development

- [x] **Mock mode** — run without an upstream API, generate responses from schemas
- [ ] **Request validation** — validate tool inputs against JSON Schema before forwarding
- [x] **Record/replay** — record real API responses for offline testing (feature-gated)

### API Lifecycle (yas-mcp as an Agent Tool)

- [ ] **`api_onboard` MCP tool** — an agent sends an OpenAPI URL, yas-mcp
  fetches it, parses it, registers tools, and returns a summary. No restart needed.
  ```json
  {"method": "tools/call", "params": {"name": "api_onboard", "arguments": {
    "spec_url": "https://api.example.com/openapi.json",
    "name": "example-api"
  }}}
  ```
- [ ] **`api_list` MCP tool** — list all onboarded APIs with tool counts
- [ ] **`api_status` MCP tool** — health + metrics for one onboarded API
- [ ] **`api_update` MCP tool** — re-fetch spec, diff tools, hot-reload changed ones
- [ ] **`api_offboard` MCP tool** — remove an API and its tools
- [ ] **Dynamic tool registry** — tools come and go without server restart.
  New OpenAPI spec arrives → tools appear. API removed → tools disappear.
  The agent loop: discover API → onboard → use tools → monitor → update → offboard.

### Operations

- [ ] **Helm chart** — for broader Kubernetes distribution
- [ ] **Multi-API instance** — one yas-mcp proxying multiple upstream APIs
- [ ] **Blue/green tool reload** — zero-downtime tool registry updates
