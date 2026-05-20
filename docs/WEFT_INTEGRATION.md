# Weft Cluster Integration

> **Status**: yas-mcp is already designed into the Weft enterprise stack — with real K8s manifests, an inline OpenAPI spec for Zypi, and a defined slot in the architecture.
> **Source**: `weft/deploy/k8s/enterprise/yas-mcp.yaml`, `weft/deploy/k8s/enterprise/README.md`

## yas-mcp's Slot in Weft (As Designed)

```
                          pi / MCP Client
                                 │
                        HTTPS + OIDC JWT
                                 │
                    ┌────────────▼────────────┐
                    │     oauth2-proxy         │
                    │     (Keycloak OIDC)      │
                    └────────────┬────────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
    ┌─────────▼─────────┐ ┌─────▼─────┐ ┌─────────▼─────────┐
    │      weft          │ │ yas-mcp  │ │      vault        │
    │     :8080          │ │  :3000   │ │      :8200        │
    │  MCP orchestration │ │OpenAPI→  │ │   secrets mgmt    │
    │                    │ │  MCP     │ │                   │
    └─────────┬──────────┘ └─────┬─────┘ └───────────────────┘
              │                  │
    ┌─────────▼──────────┐ ┌─────▼─────┐
    │       zypi         │ │  mosaic   │
    │      :4000         │ │  :4040    │
    └────────────────────┘ └───────────┘
```

## Defined Role

From weft's enterprise README:

> **yas-mcp**: Auto-generate MCP tools from Zypi OpenAPI spec

yas-mcp is the **API surface layer** — it turns Zypi's raw REST API into MCP tools that agents can call. Without yas-mcp, agents would need to know Zypi's HTTP endpoints, request formats, and parameters. With yas-mcp, they get typed MCP tools: `zypi_exec`, `zypi_session_create`, etc.

## What yas-mcp Proxies (Today)

The `yas-mcp-zypi-spec` ConfigMap in `weft/deploy/k8s/enterprise/yas-mcp.yaml` contains an inline OpenAPI 3.0 spec for Zypi's API:

| MCP Tool | Zypi Endpoint | Purpose |
|----------|--------------|---------|
| `zypi_exec` | POST /exec | One-shot sandbox execution |
| `zypi_session_create` | POST /sessions | Create long-lived sandbox session |
| `zypi_session_exec` | POST /sessions/{id}/exec | Execute in existing session |
| `zypi_session_get` | GET /sessions/{id} | Get session details |
| `zypi_session_close` | DELETE /sessions/{id} | Close and destroy session |
| `zypi_image_warm` | POST /images/{ref}/warm | Pre-warm VMs for an image |
| `zypi_pool_stats` | GET /pool/stats | VM pool statistics |
| `zypi_health` | GET /health | Zypi health check |

## Future: More Proxied APIs

The same pattern can extend to other weft services:

### MosaicDB (Proposed)

```yaml
# ConfigMap: yas-mcp-mosaic-spec
data:
  openapi.yaml: |
    paths:
      /api/search:
        post:
          operationId: mosaic_search
          summary: Semantic vector search
      /api/memory/remember:
        post:
          operationId: mosaic_memory_remember
          summary: Store agent memory
      /api/memory/recall:
        post:
          operationId: mosaic_memory_recall
          summary: Recall agent memories
      # ... 15+ more endpoints
```

Generates: `mosaic_search`, `mosaic_memory_remember`, `mosaic_memory_recall`, etc.

### FlowEngine (Proposed)

```yaml
# ConfigMap: yas-mcp-flowengine-spec
data:
  openapi.yaml: |
    paths:
      /api/workflows:
        post:
          operationId: flowengine_create_workflow
          summary: Create a DAG workflow
      /api/workflows/{id}/execute:
        post:
          operationId: flowengine_execute_workflow
          summary: Execute a workflow
      # ... etc
```

Generates: `flowengine_create_workflow`, `flowengine_execute_workflow`, etc.

## K8s Deployment (Actual)

```yaml
# From weft/deploy/k8s/enterprise/yas-mcp.yaml
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: yas-mcp-zypi-spec    # ← OpenAPI spec injected here
  namespace: weft
data:
  openapi.yaml: |
    openapi: "3.0.3"
    info:
      title: Zypi Agent Sandbox API
      version: "1.0.0"
    servers:
      - url: http://zypi:4000
    paths:
      /exec:
        post:
          operationId: zypi_exec
          # ... full spec inline
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: yas-mcp
  namespace: weft
spec:
  containers:
  - name: yas-mcp
    image: ghcr.io/allen-munsch/yas-mcp:latest
    args:
      - --swagger-file /config/openapi.yaml
      - --mode http
      - --host 0.0.0.0
      - --port 3000
      - --endpoint http://zypi:4000
    volumeMounts:
    - name: spec
      mountPath: /config
      readOnly: true
  volumes:
  - name: spec
    configMap:
      name: yas-mcp-zypi-spec
```

## Authentication Flow

```
pi
 │
 │ 1. tools/call (no auth)
 ▼
oauth2-proxy (:4180)
 │
 │ 2. 302 → Keycloak login
 ▼
keycloak (:8080)
 │
 │ 3. Login → JWT issued
 ▼
oauth2-proxy (:4180)
 │
 │ 4. JWT forwarded → weft or yas-mcp
 ▼
weft (:8080)  ←→  yas-mcp (:3000)
```

Agents authenticate via Keycloak OIDC. The oauth2-proxy sits in front of both weft and yas-mcp, enforcing JWT auth. yas-mcp itself doesn't need to implement OIDC (Phase 1) when deployed in the weft stack — the proxy handles it. yas-mcp's OIDC is for **standalone deployments**.

## Weft Agent Conventions

### Communication via INBOX/OUTBOX

yas-mcp follows the [weft Factorio-belt model](https://github.com/allen-munsch/weft/blob/main/shared/AGENT-CONVENTIONS.md):

```
weft/shared/
├── INBOX-yas-mcp.md         ← Tasks from weft to yas-mcp agent
├── OUTBOX-yas-mcp.md        ← yas-mcp completed work/decisions
│
yas-mcp/shared/              ← yas-mcp's side of the belt
├── INBOX-weft.md            ← Tasks from yas-mcp to weft agent
├── OUTBOX-weft.md           ← Weft's responses
```

### Proposing Changes to Weft

When yas-mcp needs weft to change something:

```bash
cd ~/projects/weft
git checkout -b yas-mcp-proposed/add-mosaic-proxy
# make changes to deploy/k8s/enterprise/yas-mcp.yaml
git push origin yas-mcp-proposed/add-mosaic-proxy
# Then file an INBOX-weft.md task referencing the branch
```

### Submodule Updates

If yas-mcp becomes a weft submodule (like zypi/mosaic/flowengine), weft would:

```bash
cd submodules/yas-mcp && git pull origin main
git add submodules/yas-mcp && git commit -m "chore: update yas-mcp submodule to <hash>"
```

## Integration Levels

| Level | Description | Status |
|-------|-------------|--------|
| **1. Deployed** | yas-mcp runs in weft namespace, generates Zypi MCP tools | ✅ Designed (manifests exist) |
| **2. Discoverable** | weft health check monitors yas-mcp, appears in `weft_config` | ⬜ Not yet |
| **3. Memory-aware** | yas-mcp tool results auto-persist to MosaicDB | ⬜ Not yet |
| **4. Sandbox-aware** | yas-mcp can route calls through Zypi Firecracker VMs | ⬜ Not yet |
| **5. Workflow-integrated** | yas-mcp tools usable in FlowEngine DAGs | ⬜ Not yet |
| **6. Agent-loop-integrated** | weft agent loop dynamically discovers/calls yas-mcp tools | ⬜ Not yet |
| **7. Belt-integrated** | yas-mcp participates in INBOX/OUTBOX communication | ⬜ Not yet |

## Phase 5 Tasks (Updated with Real Analysis)

- [x] **K8s manifest exists** — `weft/deploy/k8s/enterprise/yas-mcp.yaml` deploys yas-mcp with Zypi spec
- [x] **Zypi OpenAPI spec defined** — inline spec in ConfigMap generates 8 MCP tools
- [ ] **Add MosaicDB spec** — create `yas-mcp-mosaic-spec` ConfigMap for MosaicDB REST → MCP
- [ ] **Add FlowEngine spec** — create `yas-mcp-flowengine-spec` ConfigMap for FlowEngine → MCP
- [ ] **Weft health registration** — yas-mcp registers with weft's health endpoint
- [ ] **MosaicDB auto-persist** — tool call results → `POST /api/memory/remember`
- [ ] **Zypi sandbox routing** — optional sandbox flag on routes (adjustments.yaml)
- [ ] **FlowEngine DAG nodes** — yas-mcp tools as FlowEngine node types
- [ ] **Weft MCP tools** — `weft_mcp_onboard`, `weft_mcp_status` for life cycle management
- [ ] **INBOX/OUTBOX files** — create `weft/shared/INBOX-yas-mcp.md` and `yas-mcp/shared/INBOX-weft.md`
- [ ] **Multi-spec support** — one yas-mcp instance proxies multiple APIs (Zypi + MosaicDB + FlowEngine)

## Quick Wins (Based on Real Analysis)

| Task | Effort | Impact |
|------|--------|--------|
| Create MosaicDB OpenAPI spec for yas-mcp | S | High — 15+ MCP tools auto-generated |
| Create FlowEngine OpenAPI spec for yas-mcp | S | High — workflow tools auto-generated |
| Add yas-mcp to weft health circuit | XS | Medium — appears in `weft_config` |
| Create `weft/shared/INBOX-yas-mcp.md` | XS | Low — enable belt communication |
| Multi-spec ConfigMap mounting | S | High — one yas-mcp proxies all weft services |
