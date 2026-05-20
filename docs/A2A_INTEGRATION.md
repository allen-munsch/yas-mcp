# A2A Protocol Integration

> **Goal**: yas-mcp speaks the Agent-to-Agent (A2A) protocol natively — any A2A agent can discover and delegate tasks to APIs proxied through yas-mcp.

## Why A2A + MCP in One Server?

| Aspect | MCP | A2A |
|--------|-----|-----|
| **Topology** | Client-server (AI ↔ tools) | Peer-to-peer (agent ↔ agent) |
| **Discovery** | `tools/list` | Agent Card at `/.well-known/agent-card.json` |
| **Execution** | `tools/call` (sync request/response) | `tasks/send` (async with lifecycle) |
| **State** | Stateless per call | Task state machine (`submitted → working → completed`) |
| **Streaming** | Not standardized (SSE deprecated) | Built-in SSE streaming updates |
| **Use case** | AI assistant calls a tool | Agent delegates work to another agent |

**yas-mcp is the universal adapter**: same tool registry → both protocols. An API surfaced once is callable by both MCP clients (Claude, etc.) and A2A agents (other autonomous agents).

## A2A Protocol Overview

```
┌──────────────┐     Agent Card       ┌──────────────┐
│  A2A Client  │ ◄─────────────────── │   yas-mcp    │
│  (another    │                      │  (A2A agent) │
│   agent)     │     tasks/send       │              │
│              │ ────────────────────►│              │
│              │                      │              │
│              │     tasks/get        │              │
│              │ ────────────────────►│              │
│              │◄──────────────────── │              │
│              │  Task state + output │              │
│              │                      │              │
│              │     tasks/cancel     │              │
│              │ ────────────────────►│              │
└──────────────┘                      └──────┬───────┘
                                             │
                                    ┌────────▼────────┐
                                    │ Upstream REST   │
                                    │ API             │
                                    └─────────────────┘
```

### Task Lifecycle

```
 submitted ──► working ──► completed
                │    │
                │    └──► failed
                │
                └──► canceled (by client)
```

Each state transition generates an event. Long-running API calls stream `working` updates.

## Agent Card Auto-Generation

yas-mcp's Agent Card is auto-generated from the tool registry:

```json
{
  "name": "yas-mcp / Todo API",
  "description": "MCP server proxying Todo REST API — 5 tools available",
  "url": "https://mcp.example.com",
  "provider": {
    "organization": "My Org",
    "url": "https://example.com"
  },
  "version": "1.0.0",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "stateTransitionHistory": true
  },
  "defaultInputModes": ["text", "text/plain"],
  "defaultOutputModes": ["text", "text/plain"],
  "skills": [
    {
      "id": "listTodos",
      "name": "List all todos",
      "description": "Retrieve all todo items with optional filtering",
      "tags": ["todo", "list", "read"],
      "examples": [
        "List all pending todos",
        "Show completed todos"
      ],
      "inputModes": ["text", "application/json"],
      "outputModes": ["text", "application/json"]
    },
    {
      "id": "createTodo",
      "name": "Create a todo",
      "description": "Create a new todo item",
      "tags": ["todo", "create", "write"],
      "examples": [
        "Create a todo: 'Buy groceries'",
        "Add task: 'Review PR #42'"
      ],
      "inputModes": ["application/json"],
      "outputModes": ["application/json"]
    }
  ]
}
```

### Mapping: MCP Tool → A2A Skill

```rust
// Auto-generated from tool registry
fn tool_to_skill(tool: &McpTool, route: &RouteConfig) -> A2ASkill {
    A2ASkill {
        id: tool.name.clone(),
        name: tool.description.clone().unwrap_or_default(),
        description: route.description.clone(),
        tags: extract_tags(&route.path, &route.method),
        examples: generate_examples(&tool.input_schema),
        input_modes: vec!["application/json".into()],
        output_modes: vec!["application/json".into()],
    }
}
```

## A2A Endpoints (Co-located with MCP)

All A2A endpoints live alongside MCP on the same axum server:

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/.well-known/agent-card.json` | GET | Agent Card (discovery) |
| `/a2a/tasks/send` | POST | Submit a task (non-streaming) |
| `/a2a/tasks/sendSubscribe` | POST | Submit task + SSE streaming updates |
| `/a2a/tasks/get` | GET | Get task status and output |
| `/a2a/tasks/cancel` | POST | Cancel a running task |
| `/a2a/tasks/pushNotification/set` | POST | Register webhook for push updates |
| `/a2a/tasks/pushNotification/get` | GET | Get current push notification config |

### Example: `tasks/send`

Request:
```json
{
  "id": "task-abc-123",
  "sessionId": "session-xyz",
  "message": {
    "role": "user",
    "parts": [
      {
        "type": "data",
        "data": {
          "skill": "createTodo",
          "parameters": {
            "title": "Buy groceries",
            "completed": false
          }
        }
      }
    ]
  }
}
```

Response (immediate for sync, polling URL for async):
```json
{
  "id": "task-abc-123",
  "sessionId": "session-xyz",
  "contextId": "ctx-456",
  "status": {
    "state": "completed",
    "timestamp": "2026-05-05T12:00:00Z"
  },
  "artifacts": [
    {
      "artifactId": "artifact-789",
      "parts": [
        {
          "type": "data",
          "data": {
            "id": 42,
            "title": "Buy groceries",
            "completed": false,
            "createdAt": "2026-05-05T12:00:00Z"
          }
        }
      ]
    }
  ]
}
```

## Implementation Plan

### Step 1: Agent Card Generator (`src/internal/a2a/agent_card.rs`)

```rust
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub capabilities: Capabilities,
    pub skills: Vec<Skill>,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
}

impl AgentCard {
    /// Generate from tool registry
    pub fn from_registry(
        server_info: &ServerInfo,
        tools: &[Tool],
        route_configs: &[RouteConfig],
    ) -> Self { ... }
    
    /// Serve as JSON
    pub fn to_json(&self) -> serde_json::Value { ... }
}
```

### Step 2: Task Store (`src/internal/a2a/task_store.rs`)

```rust
pub struct TaskStore {
    tasks: DashMap<String, Task>,
}

pub struct Task {
    pub id: String,
    pub session_id: String,
    pub state: TaskState,
    pub skill: String,
    pub input: Value,
    pub output: Option<TaskOutput>,
    pub created_at: Instant,
    pub updated_at: Instant,
}

pub enum TaskState {
    Submitted,
    Working,
    Completed,
    Failed { error: String },
    Canceled,
}
```

### Step 3: A2A Router (`src/internal/a2a/router.rs`)

```rust
// Add to axum Router in serve_http():
let a2a_routes = Router::new()
    .route("/.well-known/agent-card.json", get(agent_card))
    .route("/a2a/tasks/send", post(tasks_send))
    .route("/a2a/tasks/sendSubscribe", post(tasks_send_subscribe))
    .route("/a2a/tasks/get", get(tasks_get))
    .route("/a2a/tasks/cancel", post(tasks_cancel));

let app = Router::new()
    .route("/health", get(health))
    .route("/mcp", post(handle_mcp_request))   // MCP
    .merge(a2a_routes);                         // A2A
```

### Step 4: Task → Tool Mapping

When an A2A `tasks/send` arrives naming a `skill`:

```
1. Parse skill name from message
2. Look up in ToolRegistry by name
3. Extract parameters from message parts
4. Call tool executor (same path as MCP tools/call)
5. Wrap result as A2A Artifact
6. Return task with completed state
```

For long-running calls: return `working` state immediately, stream updates via SSE.

## Dual-Stack Server

```yaml
server:
  mode: http               # HTTP mode enables both MCP + A2A
  host: 0.0.0.0
  port: 3000

a2a:
  enabled: true
  agent_card:
    name: "yas-mcp / Corporate APIs"
    description: "Proxied corporate REST APIs available for agent delegation"
    provider:
      organization: "My Org"
      url: "https://example.com"
  task_ttl: 3600           # seconds before completed tasks are pruned
  max_concurrent_tasks: 100
```

### Why Not a Separate Repo?

1. **Single source of truth**: The tool registry is the canonical list. Duplicating it across repos creates drift.
2. **Same dependencies**: reqwest, axum, serde, tokio — already compiled, zero additional binary size.
3. **Co-located endpoints**: Same port, same process, same health check.
4. **Unified auth**: Phase 1 OIDC protects both MCP and A2A endpoints identically.
5. **Unified telemetry**: Phase 4 traces span both protocols in the same request flow.

The A2A layer is ~500 lines of Rust on top of the existing codebase. A separate repo would need its own HTTP stack, auth, config, deployment, and CI — easily 10x the work for the same result.
