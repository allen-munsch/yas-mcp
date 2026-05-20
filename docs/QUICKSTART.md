# 🚀 Quickstart — 60 Seconds to Your First MCP Server

## 1. Clone and Start

```bash
git clone https://github.com/allen-munsch/yas-mcp.git
cd yas-mcp
docker compose up -d
```

That's it. You now have an MCP server running on `http://localhost:3000`.

## 2. Verify It's Alive

```bash
# Health check
curl http://localhost:3000/health
# → OK

# See what tools are available
curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | jq '.result.tools[].name'

# You'll see something like:
# "get_users_me"
# "get_projects"
# "post_projects"
# "get_health"
# ...
```

## 3. Call a Tool

```bash
curl -s -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc":"2.0",
    "id":2,
    "method":"tools/call",
    "params":{"name":"get_health","arguments":{}}
  }' | jq
```

## 4. Use Your Own API

Edit `config.yaml` to point at your OpenAPI spec:

```yaml
swagger_file: "your-api.yaml"
endpoint:
  base_url: "https://your-api.example.com"
```

Or use an environment variable:

```bash
export SWAGGER_FILE_PATH=~/my-api/openapi.yaml
export API_BASE_URL=https://your-api.example.com
docker compose up -d
```

## 5. Run the Flying Probe

```bash
# Comprehensive system verification
make probe

# Or directly:
bash scripts/flying-probe.sh
```

## 6. Connect Your AI Assistant

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "yas-mcp": {
      "command": "yas-mcp",
      "args": [
        "--swagger-file", "https://your-api.example.com/openapi.json",
        "--mode", "stdio"
      ]
    }
  }
}
```

### Any MCP Client

Connect to `http://localhost:3000/mcp` (HTTP mode) or use the binary directly (STDIO mode).

## 7. Enable Cool Stuff

### A2A Protocol (Agent-to-Agent)

```yaml
a2a:
  enabled: true
  agent_card_name: "My Awesome API"
```

Now your API is discoverable by other A2A agents at `/.well-known/agent-card.json`.

### Metrics

Visit `http://localhost:3000/metrics` — you'll see Prometheus metrics for every tool call, latency histogram, and error count.

### Auth

```yaml
oauth:
  enabled: true
  provider: oidc
  issuer_url: "https://auth.example.com"   # ← just the URL, auto-discovered
  client_id: "${OIDC_CLIENT_ID}"
  client_secret: "env://OIDC_CLIENT_SECRET"
```

### Response Caching

```yaml
cache:
  enabled: true
  default_ttl_secs: 60
```

---

## Common Use Cases

### "I have a REST API and want AI tools"

```bash
yas-mcp --swagger-file my-api.yaml --mode http
```

### "I want to protect my API with OAuth"

```yaml
oauth:
  enabled: true
  provider: oidc
  issuer_url: "https://your-idp.example.com"
```

### "I want agents to delegate tasks to each other"

```yaml
a2a:
  enabled: true
```

### "I want to deploy to Kubernetes"

```bash
kubectl apply -k deploy/minikube/base
```

### "I want to see what's happening"

```bash
curl http://localhost:3000/metrics | grep yas_mcp
```

---

## Next Steps

- [Architecture Overview](ARCHITECTURE.md) — how everything fits together
- [Configuration Reference](../config.yaml.example) — all options
- [Flying Probe](../scripts/flying-probe.sh) — comprehensive testing
- [Kubernetes Deployment](../deploy/minikube/base) — Kustomize manifests
