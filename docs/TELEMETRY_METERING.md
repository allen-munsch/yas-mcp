# Telemetry, Metering & Control Plane

> **Goal**: Replace Kong/Mulesoft/Envoy complexity with a lightweight Rust proxy that has first-class observability, metering, and control.

## Why Replace Kong/Mulesoft?

| Concern | Kong / Mulesoft / Envoy | yas-mcp |
|---------|------------------------|---------|
| **Binary size** | 100MB+ (LuaJIT + plugins) | ~15MB (Rust, static) |
| **Config format** | Custom DSL (Kong), YAML (Envoy) | Familiar YAML + env vars |
| **API spec awareness** | None (raw HTTP proxy) | **OpenAPI-native** — understands routes, params, schemas |
| **Plugin model** | Lua (Kong), WASM (Envoy) | **Rust modules** — type-safe, compile-time checked |
| **MCP/A2A native** | No | Yes — additional transport layer, not separate proxy |
| **Agent integration** | None | First-class Weft cluster citizen |
| **Tool generation** | Manual | **Automatic** from OpenAPI spec |

## Telemetry Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        yas-mcp                              │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Middleware Stack (tower)                │   │
│  │                                                     │   │
│  │  Request → [Trace] → [Meter] → [RateLimit] →        │   │
│  │            [Auth] → [Cache] → [Circuit] → Upstream  │   │
│  │                                                     │   │
│  │  Response ← [Trace] ← [Meter] ← [Cache] ← ...       │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  Traces  │  │ Metrics  │  │  Audit   │  │  Alerts  │   │
│  │  (OTLP)  │  │(Prometheus)│ │  (JSON)  │  │ (Webhook)│   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## 1. OpenTelemetry Traces

### W3C Traceparent Propagation

```rust
// Every outgoing HTTP request gets trace context
// yas-mcp receives: traceparent: 00-{trace_id}-{span_id}-01
// yas-mcp forwards: traceparent: 00-{trace_id}-{new_span_id}-01

// Tower middleware:
async fn trace_middleware<B>(
    req: Request<B>,
    next: Next<B>,
) -> Response {
    let traceparent = req.headers().get("traceparent");
    let span = if let Some(tp) = traceparent {
        // Continue existing trace
        tracer.span_builder("yas-mcp.tool_call")
            .with_traceparent(tp)
            .start(&tracer)
    } else {
        // Start new trace
        tracer.span_builder("yas-mcp.tool_call").start(&tracer)
    };
    
    let response = next.run(req).await;
    
    span.set_attribute("http.status_code", response.status().as_u16());
    span.set_attribute("yas-mcp.tool", tool_name);
    span.end();
    
    response
}
```

### Span Structure

```
request_received
  └── mcp.method = "tools/call"
      ├── tool_name = "listTodos"
      ├── auth.check
      │   └── oidc.provider = "corporate-sso"
      ├── rate_limit.check
      │   └── tokens_available = 95
      ├── cache.lookup
      │   └── cache_hit = false
      ├── upstream_request
      │   ├── http.method = "GET"
      │   ├── http.url = "http://todo-api:8080/todos"
      │   ├── http.status_code = 200
      │   └── http.duration_ms = 45
      ├── response_transform
      └── response_sent
```

### OTLP Export

```yaml
telemetry:
  tracing:
    enabled: true
    exporter: otlp                    # otlp | jaeger | zipkin | stdout
    endpoint: http://jaeger:4317       # OTLP gRPC
    # endpoint: http://jaeger:4318     # OTLP HTTP
    sample_rate: 1.0                  # 0.0-1.0 (0.1 = 10%)
    batch_size: 256
```

## 2. Prometheus Metrics

### `GET /metrics` Endpoint

```
# HELP yas_mcp_tool_calls_total Total tool calls
# TYPE yas_mcp_tool_calls_total counter
yas_mcp_tool_calls_total{tool="listTodos", status="success"} 1042
yas_mcp_tool_calls_total{tool="listTodos", status="error"} 3
yas_mcp_tool_calls_total{tool="createTodo", status="success"} 521

# HELP yas_mcp_tool_duration_seconds Tool call duration
# TYPE yas_mcp_tool_duration_seconds histogram
yas_mcp_tool_duration_seconds_bucket{tool="listTodos", le="0.01"} 200
yas_mcp_tool_duration_seconds_bucket{tool="listTodos", le="0.05"} 800
yas_mcp_tool_duration_seconds_bucket{tool="listTodos", le="0.1"} 950
yas_mcp_tool_duration_seconds_bucket{tool="listTodos", le="0.5"} 1040
yas_mcp_tool_duration_seconds_bucket{tool="listTodos", le="+Inf"} 1042
yas_mcp_tool_duration_seconds_sum{tool="listTodos"} 52.3
yas_mcp_tool_duration_seconds_count{tool="listTodos"} 1042

# HELP yas_mcp_upstream_health Upstream API health
# TYPE yas_mcp_upstream_health gauge
yas_mcp_upstream_health{api="todo-api"} 1
yas_mcp_upstream_health{api="erp-api"} 0

# HELP yas_mcp_circuit_breaker_state Circuit breaker state
# TYPE yas_mcp_circuit_breaker_state gauge
yas_mcp_circuit_breaker_state{api="todo-api"} 0  # 0=closed, 1=open, 2=half-open

# HELP yas_mcp_active_sessions Active MCP sessions
# TYPE yas_mcp_active_sessions gauge
yas_mcp_active_sessions 12

# HELP yas_mcp_rate_limit_remaining Rate limit tokens remaining
# TYPE yas_mcp_rate_limit_remaining gauge
yas_mcp_rate_limit_remaining{client="agent-01", tool="listTodos"} 95
```

### Metric Categories

| Category | Metrics | Type |
|----------|---------|------|
| **Throughput** | `tool_calls_total`, `upstream_requests_total` | Counter |
| **Latency** | `tool_duration_seconds`, `upstream_duration_seconds` | Histogram |
| **Errors** | `tool_errors_total`, `upstream_errors_total` | Counter |
| **Control** | `circuit_breaker_state`, `rate_limit_remaining` | Gauge |
| **Health** | `upstream_health`, `active_sessions` | Gauge |
| **Metering** | `quota_remaining`, `quota_usage_total` | Gauge/Counter |

## 3. Rate Limiting

### Token Bucket Algorithm

```rust
pub struct RateLimiter {
    buckets: DashMap<String, TokenBucket>,  // key = client_id:tool_name
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,  // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    pub fn check(&self, client: &str, tool: &str) -> RateLimitResult {
        let key = format!("{}:{}", client, tool);
        let mut bucket = self.buckets.entry(key)
            .or_insert_with(|| TokenBucket::new(100.0, 10.0)); // 100 burst, 10/sec refill
        
        bucket.refill();
        
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            RateLimitResult::Allowed { remaining: bucket.tokens as u64 }
        } else {
            RateLimitResult::Denied { retry_after: bucket.time_until_next_token() }
        }
    }
}
```

### Config

```yaml
rate_limiting:
  enabled: true
  default:
    requests_per_second: 10
    burst_size: 100
  
  per_tool:
    createTodo:
      requests_per_second: 5
      burst_size: 50
  
  per_client:
    agent-01:
      requests_per_second: 50
      burst_size: 500
```

### Response Headers

```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1715000000
Retry-After: 6  (when denied)
```

## 4. Quota Management

```yaml
quotas:
  enabled: true
  
  default:
    daily: 10000      # 10k calls/day
    monthly: 250000   # 250k calls/month
  
  per_client:
    agent-01:
      daily: 50000
      monthly: 1000000
  
  alerts:
    soft_limit_percent: 80    # Alert at 80% usage
    hard_limit_action: block  # block | warn | throttle
    grace_period: 100         # Extra calls allowed after hard limit
```

```
GET /quotas/status
{
  "client": "agent-01",
  "period": "daily",
  "used": 48200,
  "limit": 50000,
  "remaining": 1800,
  "percent": 96.4,
  "status": "warning",    // ok | warning | exceeded
  "resets_at": "2026-05-06T00:00:00Z"
}
```

## 5. Circuit Breakers

```rust
pub enum CircuitState {
    Closed,           // Normal operation
    Open,             // Failing, reject immediately
    HalfOpen,         // Testing recovery
}

pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    failure_threshold: u32,    // 3 failures → open
    success_threshold: u32,    // 2 successes → close
    cooldown: Duration,        // 30s before half-open
    last_failure: Option<Instant>,
    // Per-upstream: one breaker per base URL
}
```

```yaml
circuit_breakers:
  enabled: true
  per_upstream:
    default:
      failure_threshold: 3
      cooldown_seconds: 30
      success_threshold: 2
    
    http://slow-api.example.com:
      failure_threshold: 5
      cooldown_seconds: 60
```

## 6. Response Caching

```yaml
caching:
  enabled: true
  backend: memory           # memory | redis
  
  per_route:
    - path: /todos
      methods: [GET]
      ttl: 60               # seconds
      key_fields: [status]  # cache key includes these params
    
    - path: /products
      methods: [GET]
      ttl: 300
      invalidate_on:         # clear cache when these are called
        - POST /products
        - PUT /products/*
        - DELETE /products/*
```

## 7. Audit Logging

```json
{
  "timestamp": "2026-05-05T12:00:00.123Z",
  "level": "audit",
  "correlation_id": "corr-abc-123",
  "trace_id": "00-4bf9...abc123-00f0-01",
  "span_id": "00f0...def",
  "event": "tool_call",
  "client": {
    "id": "agent-01",
    "ip": "10.0.0.5",
    "session_id": "sess-xyz"
  },
  "request": {
    "tool": "createTodo",
    "method": "POST",
    "path": "/todos",
    "params_summary": "{\"title\": \"...\", \"completed\": false}",
    "params_hash": "sha256:abc123"
  },
  "response": {
    "status_code": 201,
    "duration_ms": 45,
    "body_size": 128
  },
  "auth": {
    "provider": "corporate-sso",
    "subject": "user@example.com"
  },
  "rate_limit": {
    "remaining": 95,
    "limit": 100
  }
}
```

## 8. Webhook Notifications

```yaml
webhooks:
  endpoints:
    - url: https://alerts.example.com/webhook
      events: [quota_exceeded, circuit_open, health_degraded]
    
    - url: https://slack.example.com/hooks/yas-mcp
      events: [quota_warning]
```

Event payload:
```json
{
  "event": "quota_exceeded",
  "client": "agent-01",
  "period": "daily",
  "usage_percent": 100,
  "limit": 50000,
  "timestamp": "2026-05-05T12:00:00Z",
  "server": "yas-mcp / Todo API"
}
```

## 9. Dashboard-Ready

All metrics are Prometheus-compatible. Drop this into any Grafana dashboard:

```yaml
# Example: Grafana dashboard panel queries
panels:
  - title: "Tool Call Rate"
    query: rate(yas_mcp_tool_calls_total[5m])
  
  - title: "P99 Latency"
    query: histogram_quantile(0.99, rate(yas_mcp_tool_duration_seconds_bucket[5m]))
  
  - title: "Error Rate"
    query: rate(yas_mcp_tool_errors_total[5m]) / rate(yas_mcp_tool_calls_total[5m])
  
  - title: "Circuit Breaker Status"
    query: yas_mcp_circuit_breaker_state
  
  - title: "Quota Usage"
    query: yas_mcp_quota_usage_percent
```

## Implementation Priority

| # | Feature | Effort | Impact | Depends On |
|---|---------|--------|--------|------------|
| 1 | Prometheus `/metrics` | S | High | Nothing |
| 2 | W3C traceparent | XS | High | Nothing |
| 3 | Rate limiting (token bucket) | S | High | Nothing |
| 4 | Circuit breakers | S | High | Nothing |
| 5 | Audit logging (JSON) | S | Medium | Nothing |
| 6 | OpenTelemetry (OTLP) | M | Medium | #2 |
| 7 | Quota management | M | Medium | #3 |
| 8 | Response caching | M | Medium | Nothing |
| 9 | Webhook notifications | S | Low | #3, #5 |

Items 1-5 can all be done in parallel. Items 1-3 are one-day features each.
