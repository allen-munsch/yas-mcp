# yas-mcp Architecture

> "Yet Another Swagger MCP" — a Rust-based bridge that automatically exposes OpenAPI/Swagger endpoints as MCP tools.

## High-Level Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                           yas-mcp                                  │
│                                                                    │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌─────────────┐  │
│  │  Config  │    │  Parser  │    │  Auth    │    │  Telemetry  │  │
│  │  Loader  │    │ (OpenAPI)│    │ (OAuth2) │    │   (WIP)     │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘    └──────┬──────┘  │
│       │               │               │                 │         │
│  ┌────▼───────────────▼───────────────▼─────────────────▼──────┐   │
│  │                     MCP Processor                           │   │
│  │  • tools/list  • tools/call  • initialize  • ping          │   │
│  └────┬────────────────────────────────────────────────┬──────┘   │
│       │                                                │          │
│  ┌────▼──────────┐                          ┌──────────▼──────┐   │
│  │  STDIO        │                          │  HTTP (axum)    │   │
│  │  Transport    │                          │  POST /mcp      │   │
│  │               │                          │  GET /health    │   │
│  └───────────────┘                          └─────────────────┘   │
│                                                                    │
└────────────────────────────────┬───────────────────────────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │   Upstream REST APIs    │
                    │  (via reqwest client)   │
                    └─────────────────────────┘
```

## Component Map

### 1. Config Layer (`src/internal/config/`)

- **`AppConfig`**: Top-level config merging YAML files, CLI args, and env vars
- **`ServerConfig`**: Host, port, mode (stdio/http)
- **`EndpointConfig`**: Upstream API base URL, auth type, custom headers
- **`OAuthConfig`**: Provider, client_id/secret, scopes, redirect URIs
- **`LoggingConfig`**: Level, format, color, output path

Config precedence: CLI args > env vars (`YAS_MCP_*`) > `config.yaml` > defaults

### 2. Parser Layer (`src/internal/parser/`)

- **`SwaggerParser`**: Reads OpenAPI 3.x specs (JSON/YAML), extracts paths + operations
- **`Adjuster`**: Applies route filtering + description overrides from `adjustments.yaml`
- **`Parser` trait**: `init()`, `parse_reader()`, `get_route_tools()` — trait-based for testability
- Output: `Vec<RouteTool>` — each containing a `RouteConfig` + `McpTool`

### 3. MCP Layer (`src/internal/mcp/`)

- **`McpProcessor`**: Pure message processor — no I/O, fully testable. Maps JSON-RPC methods → MCP protocol
- **`ToolRegistry`**: Thread-safe `HashMap<String, RegisteredTool>` with read/write locks
- **`protocol`**: JSON-RPC 2.0 types, MCP method enum

### 4. Requester Layer (`src/internal/requester/`)

- **`HttpRequester`**: Reqwest-based HTTP client with header injection, timeout management
- **`RouteExecutor`**: Closure that takes JSON params → executes HTTP request → returns `HttpResponse`
- **`RouteConfig`**: Path, method, description, headers, parameters, method-specific config (query_params, header_params, form_fields)

### 5. Auth Layer (`src/internal/auth/`)

- **`OAuth2ProviderConfig`**: Provider-specific auth/token/userinfo URLs
- Provider presets: GitHub, Google, Microsoft, Generic
- Integration with Keycloak for local development
- Currently: config-driven, no runtime OIDC discovery

### 6. Server Layer (`src/internal/server/`)

- **`Server`**: Main struct holding config, parser, requester, tool_handler
- **`ToolHandler`**: Creates tool executors, manages auth gating, converts MCP args → HTTP calls
- **STDIO mode**: Raw stdin/stdout JSON-RPC loop via `TransportRunner`
- **HTTP mode**: Axum server with `POST /mcp` (JSON-RPC) and `GET /health`

### 7. Transport Layer (`src/internal/transport/`)

- **`StdioTransport`**: Reads line-delimited JSON from stdin, writes to stdout
- **`TransportRunner`**: Loop that reads → processes → writes
- **`mock`**: Mock transport for testing

## Data Flow

```
1. OpenAPI Spec (YAML/JSON)
       ↓
2. SwaggerParser.parse() → Vec<RouteTool>
       ↓
3. For each RouteTool:
   - HttpRequester.build_route_executor() → RouteExecutor closure
   - ToolHandler.create_handler() → ToolExecutor closure
   - ToolRegistry.register(name, tool + executor)
       ↓
4. MCP Client connects (STDIO or HTTP)
       ↓
5. tools/list → ToolRegistry.list_metadata()
   tools/call → ToolRegistry.get(name).executor(args)
       ↓
6. executor(args) → RouteExecutor(JSON params) → HTTP request to upstream API
       ↓
7. HTTP response → MCP CallToolResult (text content)
```

## Key Design Decisions

1. **Traits for testability**: `Parser` trait enables mock parsers in tests. `RouteExecutor` as `Arc<dyn Fn>` enables mock HTTP clients.
2. **Thread-safe tool registry**: `RwLock<HashMap>` allows concurrent reads during tool listing with write locking only during registration.
3. **Separation of concerns**: `McpProcessor` (pure logic) ≠ `TransportRunner` (I/O) ≠ `Server` (orchestration).
4. **Config over code**: Everything configurable via YAML, env vars, or CLI — no hardcoded provider URLs or routes.
5. **HTTP-first, STDIO for AI**: HTTP mode for infrastructure, STDIO mode for direct AI assistant integration.

## Current Limitations

| Limitation | Impact | Phase |
|------------|--------|-------|
| No OIDC discovery | Manual provider config | 1 |
| No telemetry | No observability in production | 4 |
| No rate limiting | Unmetered API access | 4 |
| No K8s manifests | Manual deployment only | 2 |
| Single OpenAPI file | No multi-file `$ref` resolution | 6 |
| No response caching | Repeated calls hit upstream | 4 |
| No streaming responses | Long API calls block | 6 |
| No circuit breakers | Upstream failures propagate | 4 |
