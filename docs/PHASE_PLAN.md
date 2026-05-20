# yas-mcp Phase Plan

> **Current State**: v0.1.0 — functional OpenAPI→MCP bridge with OAuth2, HTTP/STDIO modes, Docker Compose, and Keycloak integration.
> **Goal**: Production-grade API proxy that replaces Kong/Mulesoft complexity while integrating with the Weft cluster for agent-driven orchestration.

---

## Phase 1: OIDC Plug-and-Play ✅ PARTIAL → NEXT

**Goal**: Make OIDC/SSO onboarding trivial — drop a config block, pick a provider, done.

- [x] OAuth2 provider configs (github, google, microsoft, generic)
- [x] Keycloak local testing scripts
- [ ] **Dynamic OIDC discovery** — auto-fetch `.well-known/openid-configuration` for any provider URL
- [ ] **OIDC provider registry** — plugin system where providers are pure config (no code changes)
- [ ] **Token caching with refresh** — automatic token lifecycle management
- [ ] **JWT validation + JWKS rotation** — verify tokens properly, support key rotation
- [ ] **Multi-tenant OIDC** — map different API routes to different auth providers
- [ ] **Session management** — token binding to MCP sessions, logout/revoke

**Files**: `src/internal/auth/`, `mcp-oauth-config.yaml`

---

## Phase 2: Minikube-First Deployment

**Goal**: `kubectl apply -k deploy/minikube` should deploy any yas-mcp server with one command.

- [ ] **Kustomize base** — namespace, configmap, deployment, service, ingress
- [ ] **ConfigMap-driven** — OpenAPI spec and adjustments injected via ConfigMap
- [ ] **Secret management** — OIDC secrets via Kubernetes Secrets (not env vars)
- [ ] **Health probes** — readiness/liveness endpoints aligned with `GET /health`
- [ ] **Resource limits** — sane defaults with tunable overlays
- [ ] **Ingress + TLS** — cert-manager annotations, Let's Encrypt support
- [ ] **Minikube quickstart** — `make minikube-deploy` with local image build + load
- [ ] **Multi-instance support** — deploy multiple yas-mcp instances (different APIs) in the same namespace
- [ ] **Helm chart** — optional Helm packaging for broader K8s distribution

**Files**: `deploy/minikube/`, `deploy/helm/`

---

## Phase 3: Agent Skills — The API Conveyor Belt

**Goal**: Any agent (or human) can say "surface this API" and get an MCP server deployed, monitored, and metered — with zero manual wiring.

- [ ] **`.pi/agents/yas-mcp.md`** — Agent definition for the yas-mcp domain expert
- [ ] **`.pi/agents/api-belt.md`** — Specialist agent: takes an OpenAPI URL → configures → deploys → registers
- [ ] **MCP tool: `api_onboard`** — accepts OpenAPI URL/OAS JSON, generates config + adjustments, deploys
- [ ] **MCP tool: `api_list`** — lists all onboarded APIs with status, tool counts, health
- [ ] **MCP tool: `api_offboard`** — removes an API, cleans up deployments
- [ ] **MCP tool: `api_update`** — re-parses OpenAPI spec, detects diff, hot-reloads tools
- [ ] **Skills package** — reusable skill files that agents import for API onboarding workflows

**Files**: `.pi/agents/`, `skills/`, `src/internal/mcp/api_belt.rs`

---

## Phase 4: Telemetry, Metering & Control Plane

**Goal**: Replace Kong/Mulesoft/Envoy complexity with a lightweight Rust proxy that has first-class observability.

- [ ] **OpenTelemetry** — traces (W3C traceparent), spans per tool call, span links to upstream
- [ ] **Metrics export** — Prometheus `/metrics` endpoint with counters, histograms, gauges
- [ ] **Request metering** — per-tool, per-client, per-API usage counters with reset periods
- [ ] **Rate limiting** — token bucket per client/IP/tool with configurable limits
- [ ] **Quota management** — daily/monthly quotas with soft/hard limits and grace periods
- [ ] **Audit logging** — structured JSON logs with correlation IDs, request/response metadata
- [ ] **Circuit breakers** — per-upstream-API circuit breakers (3 failures → open, 30s cooldown)
- [ ] **Response caching** — configurable TTL cache per route, cache invalidation hooks
- [ ] **API analytics** — histograms of latency, error rates, throughput per tool
- [ ] **Webhook notifications** — quota exceeded, circuit open, health degraded

**Files**: `src/internal/telemetry/`, `src/internal/metering/`, `src/internal/control/`

---

## Phase 5: Weft Cluster Integration

**Goal**: yas-mcp is already designed into the Weft enterprise stack — with real K8s manifests (`weft/deploy/k8s/enterprise/yas-mcp.yaml`), an inline OpenAPI spec for Zypi, and a defined role as the **API surface layer**. This phase makes it fully operational and extends coverage to all weft services.

**Defined slot** (from weft's enterprise architecture): pi → oauth2-proxy (Keycloak OIDC) → weft:8080 + yas-mcp:3000. yas-mcp auto-generates MCP tools from OpenAPI specs injected as ConfigMaps.

- [x] **K8s manifest exists** — `weft/deploy/k8s/enterprise/yas-mcp.yaml` deploys yas-mcp, mounts `yas-mcp-zypi-spec` ConfigMap
- [x] **Zypi OpenAPI spec defined** — inline spec in ConfigMap generates 8 MCP tools (`zypi_exec`, `zypi_session_create`, `zypi_session_exec`, `zypi_session_get`, `zypi_session_close`, `zypi_image_warm`, `zypi_pool_stats`, `zypi_health`)
- [ ] **Add MosaicDB spec** — create `yas-mcp-mosaic-spec` ConfigMap for MosaicDB REST → 15+ MCP tools
- [ ] **Add FlowEngine spec** — create `yas-mcp-flowengine-spec` ConfigMap for FlowEngine → MCP tools
- [ ] **Multi-spec support** — one yas-mcp instance proxies multiple APIs (Zypi + MosaicDB + FlowEngine simultaneously)
- [ ] **Weft health registration** — yas-mcp registers with weft's health endpoint, appears in `weft_config` and circuit breakers
- [ ] **MosaicDB auto-persist** — tool call results → `POST /api/memory/remember` for agent recall
- [ ] **Zypi sandbox routing** — optional per-route sandbox flag for isolated execution
- [ ] **FlowEngine DAG integration** — yas-mcp tools as FlowEngine node types for workflow composition
- [ ] **Weft MCP tools** — `weft_mcp_onboard`, `weft_mcp_status`, `weft_mcp_call`, `weft_mcp_offboard`
- [ ] **INBOX/OUTBOX** — create `weft/shared/INBOX-yas-mcp.md` and `yas-mcp/shared/INBOX-weft.md`
- [ ] **Agent loop** — weft agent loop dynamically discovers and calls yas-mcp tools via MCP

**Files**: `weft/deploy/k8s/enterprise/yas-mcp.yaml`, `src/internal/weft/`, `shared/`

---

## Phase 6: A2A Protocol — Agent-to-Agent Interoperability

**Goal**: yas-mcp speaks the A2A protocol natively — any A2A agent can discover and delegate tasks to APIs proxied through yas-mcp.

A2A ([a2a-protocol.org](https://a2a-protocol.org/latest/specification/)) is Google's open standard for agent-to-agent communication. Where MCP is client-server (AI client ↔ tool server), A2A is peer-to-peer (agent ↔ agent). yas-mcp implements both as transport modes on the same tool registry.

- [ ] **A2A Agent Card generation** — auto-generate A2A Agent Cards from the tool registry
- [ ] **A2A transport** — implement `tasks/send`, `tasks/get`, `tasks/cancel` endpoints
- [ ] **Task lifecycle** — `submitted → working → completed/failed/canceled` with streaming updates
- [ ] **A2A skill mapping** — each MCP tool becomes an A2A skill in the Agent Card
- [ ] **A2A push notifications** — webhook-based task status push to requesting agents
- [ ] **Multi-agent delegation** — yas-mcp can delegate sub-tasks to other A2A agents
- [ ] **A2A + MCP dual-stack** — same server exposes both MCP and A2A endpoints simultaneously
- [ ] **A2A artifact exchange** — structured output artifacts (JSON, files) across agent boundaries
- [ ] **A2A authentication** — align with Phase 1 OIDC for agent identity verification
- [ ] **A2A streaming** — Server-Sent Events for long-running API calls as A2A tasks

**Files**: `src/internal/a2a/`, `src/internal/a2a/agent_card.rs`, `src/internal/a2a/task_handler.rs`

**Spec reference**: [A2A Protocol v1.0](https://a2a-protocol.org/latest/specification/)

---

## Phase 7: AI Catalog — Universal Discovery

**Goal**: yas-mcp auto-publishes an AI Catalog entry — making every proxied API discoverable through the emerging cross-protocol AI Catalog standard.

The [AI Catalog](https://github.com/Agent-Card/ai-catalog) is a collaboration between MCP, A2A, and other protocol communities to create a common discovery layer for AI artifacts. yas-mcp generates catalog entries for every onboarded API.

- [ ] **Auto-generate AI Catalog** — from tool registry, produce a standards-compliant catalog entry
- [ ] **`.well-known/ai-catalog.json`** — serve at well-known URL for domain-based discovery
- [ ] **Nested catalog entries** — one yas-mcp instance → multiple API surfaces → multiple catalog entries
- [ ] **Trust Manifest** — optional identity, attestations, and provenance metadata per entry
- [ ] **Multi-format output** — same tool exposed as MCP tool, A2A skill, and AI Catalog entry
- [ ] **Catalog update hooks** — push updates to registries when tools change (onboard/offboard/update)
- [ ] **Registry integration** — compatible with A2A Registry and MCP Registry standards

**Files**: `src/internal/catalog/`, `src/internal/catalog/well_known.rs`

**Spec reference**: [Agent-Card/ai-catalog](https://github.com/Agent-Card/ai-catalog)

---

## Phase 8: Production Hardening

**Goal**: Enterprise-grade reliability, security, and operability.

- [ ] **gRPC transport** — alternative to JSON-RPC over HTTP for high-throughput
- [ ] **WebSocket streaming** — streaming tool responses for long-running API calls
- [ ] **Multi-file OpenAPI** — resolve `$ref` across multiple spec files
- [ ] **Response transformation** — JSONPath/jq-style response filtering before returning to client
- [ ] **Request validation** — JSON Schema validation of tool inputs before forwarding
- [ ] **API version negotiation** — detect API version changes, graceful migration
- [ ] **Blue/green deployment** — zero-downtime tool reload with versioned tool registries
- [ ] **Chaos testing** — integration with weft's sandbox for chaos engineering
- [ ] **SOC2/ISO27001 controls** — audit trails, access controls, encryption at rest

---

## Unified Architecture: MCP + A2A + AI Catalog

```
                         ┌──────────────────────────┐
  OpenAPI Spec ─────────►│                          │
                         │     Tool Registry        │
  adjustments.yaml ─────►│  (RouteConfig + McpTool) │
                         │                          │
                         └──────────┬───────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    │               │               │
              ┌─────▼─────┐  ┌──────▼──────┐  ┌─────▼─────┐
              │ MCP Layer │  │ A2A Layer   │  │  Catalog  │
              │           │  │             │  │  Layer    │
              │ • tools/  │  │ • Agent     │  │           │
              │   list    │  │   Card      │  │ • .well-  │
              │ • tools/  │  │ • tasks/    │  │   known/  │
              │   call    │  │   send      │  │   ai-     │
              │ • STDIO   │  │ • tasks/    │  │   catalog │
              │ • HTTP    │  │   get       │  │   .json   │
              │   POST    │  │ • tasks/    │  │           │
              │   /mcp    │  │   cancel    │  │ • Trust   │
              │           │  │             │  │   Manifest│
              └───────────┘  └─────────────┘  └───────────┘
                    │               │               │
                    ▼               ▼               ▼
              MCP Clients    A2A Agents     AI Registries
              (Claude, etc)  (Other agents) (Catalogs,
                                            Marketplaces)
                                  │
                    ┌─────────────┴─────────────┐
                    │   Upstream REST APIs       │
                    │   (via reqwest client)     │
                    └───────────────────────────┘
```

**Key insight**: The tool registry is the single source of truth. MCP, A2A, and AI Catalog are three different *representations* of the same underlying API surface. No duplication, no drift.

---

## Phase Dependencies

```
Phase 1 (OIDC) ──────┐
                      ├──► Phase 3 (Agent Skills) ──► Phase 5 (Weft Integration)
Phase 2 (Minikube) ───┤                              │
                      ├──► Phase 4 (Telemetry) ──────┤
                      │                              │
                      ├──► Phase 6 (A2A Protocol) ───┤
                      │                              │
                      └──► Phase 7 (AI Catalog) ─────┘
                                                      │
                                                      └──► Phase 8 (Hardening)
```

- Phase 1 & 2 are parallel — OIDC doesn't block K8s deployment
- Phase 3 depends on Phase 1 (auth) but can start in parallel with stubs
- Phase 4 can start immediately — telemetry is independent
- **Phase 6 (A2A) can start immediately** — it's another transport layer on top of the existing tool registry
- **Phase 7 (AI Catalog) depends on Phase 6** — A2A Agent Cards inform catalog entries
- Phase 5 depends on Phases 3 & 4 — needs agent skills AND telemetry for full integration
- Phase 8 depends on Phase 5 — production hardening after weft integration is proven

---

## Quick Wins (Parallelizable First Tasks)

| Task | Effort | Impact | Phase |
|------|--------|--------|-------|
| OIDC discovery from `.well-known` | S | High | 1 |
| Minikube Kustomize base | S | High | 2 |
| Prometheus `/metrics` endpoint | S | High | 4 |
| `.pi/agents/yas-mcp.md` agent def | XS | Medium | 3 |
| `api_list` MCP tool | S | Medium | 3 |
| W3C traceparent propagation | XS | High | 4 |
| **A2A Agent Card auto-generation** | S | High | 6 |
| **`.well-known/ai-catalog.json` endpoint** | S | Medium | 7 |
| Health registration with weft | S | Medium | 5 |
| MosaicDB auto-persist tool results | M | High | 5 |
| **A2A `tasks/send` endpoint** | M | High | 6 |
