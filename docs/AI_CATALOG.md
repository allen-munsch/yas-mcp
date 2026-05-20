# AI Catalog Integration

> **Goal**: yas-mcp auto-publishes AI Catalog entries — every proxied API is discoverable through the emerging cross-protocol AI Catalog standard.

## What Is the AI Catalog?

The [AI Catalog](https://github.com/Agent-Card/ai-catalog) is a collaboration between the MCP, A2A, and broader AI protocol communities. It defines a **common discovery layer** for heterogeneous AI artifacts — MCP servers, A2A agent cards, Claude Code plugins, datasets, model cards, and nested catalogs.

Key design principles:
- **Protocol-agnostic**: Entries reference artifacts by media type, not by protocol
- **Nestable**: Catalogs can contain sub-catalogs
- **Well-known URL**: `/.well-known/ai-catalog.json` for domain-based discovery
- **Trust Manifest** (optional): Identity, attestations, provenance metadata

## yas-mcp as a Catalog Publisher

yas-mcp is uniquely positioned: it already knows about every API surface it proxies. It can auto-generate catalog entries for:

1. **The yas-mcp server itself** — as an MCP server and A2A agent
2. **Each onboarded API** — as individual catalog entries
3. **The aggregate** — as a nested catalog

```
/.well-known/ai-catalog.json
{
  "entries": [
    {
      "id": "yas-mcp-todo-api",
      "type": "mcp_server",
      "name": "Todo API via yas-mcp",
      "artifact": { "mediaType": "application/json", "url": "/mcp" },
      "description": "MCP server proxying Todo REST API"
    },
    {
      "id": "yas-mcp-todo-api-a2a",
      "type": "a2a_agent_card",
      "name": "Todo API Agent (A2A)",
      "artifact": { "mediaType": "application/json", "url": "/.well-known/agent-card.json" }
    },
    {
      "id": "yas-mcp-petstore",
      "type": "mcp_server",
      "name": "Petstore API via yas-mcp",
      "artifact": { "mediaType": "application/json", "url": "/petstore/mcp" }
    }
  ]
}
```

## Catalog Entry Structure

```json
{
  "id": "yas-mcp-todo-api",
  "mediaType": "application/vnd.ai.catalog.entry+json",
  "type": "mcp_server",
  "name": "Todo REST API",
  "description": "MCP server proxying Todo REST API — 5 tools for CRUD operations",
  "publisher": {
    "name": "My Org",
    "url": "https://example.com"
  },
  "version": "1.0.0",
  "artifact": {
    "mediaType": "application/json",
    "url": "https://mcp.example.com/mcp",
    "digest": {
      "algorithm": "sha256",
      "value": "abc123..."
    }
  },
  "documentation": {
    "url": "https://docs.example.com/todo-api",
    "description": "Todo API documentation"
  },
  "icons": [
    {
      "src": "https://example.com/todo-icon.png",
      "sizes": "64x64",
      "type": "image/png"
    }
  ],
  "trust": {
    "identity": {
      "did": "did:web:mcp.example.com",
      "spiffe": "spiffe://example.com/yas-mcp/todo-api"
    },
    "attestations": [
      {
        "type": "security-review",
        "url": "https://example.com/audit/todo-api-2026.pdf",
        "timestamp": "2026-01-15T00:00:00Z"
      }
    ]
  },
  "entry": {
    "mediaType": "application/json",
    "url": "https://mcp.example.com/catalog/entries/todo-api"
  }
}
```

## Auto-Generation from Tool Registry

```rust
// src/internal/catalog/generator.rs

pub struct CatalogGenerator {
    server_info: ServerInfo,
    tool_registry: Arc<ToolRegistry>,
    route_configs: Vec<RouteConfig>,
}

impl CatalogGenerator {
    /// Generate a full AI Catalog from current state
    pub fn generate(&self) -> AiCatalog {
        let entries: Vec<CatalogEntry> = self.route_configs
            .iter()
            .map(|route| self.route_to_entry(route))
            .collect();

        AiCatalog {
            entries,
            // Always include the A2A agent card as a top-level entry
            // and the MCP endpoint as well
        }
    }

    /// Generate a single entry from a route config
    fn route_to_entry(&self, route: &RouteConfig) -> CatalogEntry {
        CatalogEntry {
            id: format!("yas-mcp-{}", slugify(&route.path)),
            media_type: "application/vnd.ai.catalog.entry+json".into(),
            entry_type: "mcp_server".into(), // or "a2a_agent_card" if both
            name: route.description.clone(),
            description: format!(
                "MCP tool for {} {} at {}",
                route.method, route.path, self.server_info.url
            ),
            artifact: ArtifactRef {
                media_type: "application/json".into(),
                url: format!("{}/mcp", self.server_info.url),
                digest: None, // compute if static
            },
            // ... other fields from config
        }
    }
}
```

## Well-Known Endpoint

```rust
// Add to axum Router
async fn well_known_catalog(
    State(state): State<AppState>,
) -> Json<Value> {
    let generator = CatalogGenerator::new(
        &state.server,
        state.tool_registry.clone(),
    );
    Json(generator.generate().to_json())
}
```

### Discovery Flow

```
1. AI Registry crawler
       │
       ▼
2. GET https://mcp.example.com/.well-known/ai-catalog.json
       │
       ▼
3. yas-mcp generates catalog from tool registry
       │
       ▼
4. Registry receives:
   - 1 MCP server entry (Todo API)
   - 1 A2A agent card entry (Todo API Agent)
   - 2 entries total for this yas-mcp instance
       │
       ▼
5. Registry indexes entries → discoverable by AI clients
```

## Nested Catalogs for Multi-API Instances

When yas-mcp proxies multiple APIs, it generates a nested catalog:

```json
{
  "entries": [
    {
      "id": "yas-mcp-root",
      "type": "ai_catalog",
      "name": "My Org API Catalog",
      "description": "All APIs surfaced via yas-mcp",
      "entry": {
        "mediaType": "application/vnd.ai.catalog+json",
        "url": "https://mcp.example.com/.well-known/ai-catalog.json"
      }
    },
    {
      "id": "yas-mcp-corp-erp",
      "type": "mcp_server",
      "name": "ERP API",
      "artifact": { "mediaType": "application/json", "url": "https://mcp.example.com/erp/mcp" }
    },
    {
      "id": "yas-mcp-corp-crm",
      "type": "mcp_server",
      "name": "CRM API",
      "artifact": { "mediaType": "application/json", "url": "https://mcp.example.com/crm/mcp" }
    }
  ]
}
```

## Integration with Phase 6 (A2A)

The AI Catalog naturally references A2A Agent Cards:

```
AI Catalog Entry
  ├── type: "a2a_agent_card"
  └── artifact.url: "/.well-known/agent-card.json"
       │
       ▼
A2A Agent Card
  ├── skills: [...]         ← same tools as MCP tools/list
  └── capabilities: {...}
       │
       ▼
A2A tasks/send
  └── Executes tool via ToolRegistry (same as MCP tools/call)
```

The catalog is a discovery pointer, the Agent Card describes capabilities, and the protocol endpoints execute them. All three layers draw from the same tool registry.

## Update Hooks

When tools change (onboarded, offboarded, updated), the catalog updates:

```rust
// In api_belt.rs after onboarding a new API
async fn onboard_api(spec_url: &str) -> Result<()> {
    // ... parse spec, register tools ...
    
    // Regenerate catalog with new entry
    catalog_generator.regenerate();
    
    // Push update to registries (if configured)
    if let Some(registry_url) = &config.catalog_registry_url {
        push_catalog_update(registry_url, &catalog).await?;
    }
}
```

## Why Not a Separate Repo?

Same reasons as A2A:

| Concern | In yas-mcp | Separate repo |
|---------|-----------|---------------|
| **Source of truth** | Tool registry already exists | Duplicate tool metadata |
| **Freshness** | Always current (same process) | Sync lag, drift risk |
| **Code** | ~200 lines of Rust | New HTTP server, auth, config, CI |
| **Deployment** | Same pod, same health check | Separate service to manage |
| **Auth** | Phase 1 OIDC covers it | Reimplement auth |
| **Telemetry** | Phase 4 traces span it | Separate observability |

The AI Catalog layer is a thin read-only view over the existing tool registry. A separate repo would reimplement the registry and never stay in sync.
