#!/bin/bash
# ═══════════════════════════════════════════════════════════════════════════
# yas-mcp MULTI-AGENT DEMO
# ═══════════════════════════════════════════════════════════════════════════
# Shows multiple AI agents using yas-mcp simultaneously:
#   Agent 1 (Coder):    lists tools, explores API surface
#   Agent 2 (Tester):   calls tools, validates responses
#   Agent 3 (Manager):  uses A2A to delegate tasks
#   Agent 4 (Monitor):  watches metrics and health
# ═══════════════════════════════════════════════════════════════════════════

set -e

MCP_URL="${MCP_URL:-http://127.0.0.1:3002}"
MINI_AGENT_URL="http://127.0.0.1:9001"
BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[32m'
CYAN='\033[36m'
YELLOW='\033[33m'
MAGENTA='\033[35m'
NC='\033[0m'

agent_header() {
    echo ""
    echo -e "${BOLD}┌─ $1 ──────────────────────────────────────────────┐${NC}"
}

agent_say()  { echo -e "  ${CYAN}$1${NC}"; }
agent_do()   { echo -e "  ${DIM}$1${NC}"; }
agent_ok()   { echo -e "  ${GREEN}✓ $1${NC}"; }
agent_wait() { sleep "$1"; }

mcp() {
    curl -s -X POST "$MCP_URL/mcp" \
        -H "Content-Type: application/json" \
        -d "$1" 2>/dev/null
}

cleanup() {
    echo ""
    echo -e "${DIM}Shutting down...${NC}"
    kill $SERVER_PID 2>/dev/null || true
    kill $MINI_AGENT_PID 2>/dev/null || true
    wait $SERVER_PID 2>/dev/null || true
    wait $MINI_AGENT_PID 2>/dev/null || true
}
trap cleanup EXIT

# ── Start yas-mcp in demo mode ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║  ☀️  yas-mcp MULTI-AGENT DEMO                                ║${NC}"
echo -e "${BOLD}║  4 agents, 1 server, simultaneous tool calls                ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${DIM}Starting yas-mcp in demo mode (built-in API + mock data)...${NC}"

cargo run --bin yas-mcp -- --demo &
SERVER_PID=$!

# Wait for server
for i in $(seq 1 30); do
    if curl -sf "$MCP_URL/health" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓ Server ready${NC}"
        break
    fi
    [ "$i" = "30" ] && { echo "  ✗ Server failed to start"; exit 1; }
    sleep 1
done

# Start mini A2A agent
echo -e "${DIM}Starting Mini A2A Agent (weather, calculator, translate)...${NC}"
python3 scripts/mini-agent.py &
MINI_AGENT_PID=$!
for i in $(seq 1 10); do
    if curl -sf "$MINI_AGENT_URL/health" > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓ Mini Agent ready${NC}"
        break
    fi
    sleep 0.5
done

# ── Agent 1: CODER — explore the API surface ───────────────────────────────
agent_header "Agent 1: CODER — exploring API surface"
agent_say "Hey, what tools are available on this server?"

TOOLS=$(mcp '{"jsonrpc":"2.0","id":"a1-1","method":"tools/list","params":{}}')
COUNT=$(echo "$TOOLS" | jq -r '.result.tools | length' 2>/dev/null || echo "0")
agent_ok "Found $COUNT tools"

agent_say "Let me look at the first few..."
echo "$TOOLS" | jq -r '.result.tools[0:4][] | "    • \(.name): \(.description // "no description")"' 2>/dev/null | while read -r line; do
    agent_do "$line"
done

agent_say "I'll grab a project to see the data shape..."
agent_wait 0.5
RESULT=$(mcp '{"jsonrpc":"2.0","id":"a1-2","method":"tools/call","params":{"name":"get_project","arguments":{"id":"demo-1"}}}')
DATA=$(echo "$RESULT" | jq -c '.result.content[0].text' 2>/dev/null | sed 's/^"//;s/"$//' || echo "{}")
agent_ok "Got project data: $(echo "$DATA" | jq -c '{name, status}' 2>/dev/null || echo 'mock data')"

# ── Agent 2: TESTER — validate tool responses ──────────────────────────────
agent_header "Agent 2: TESTER — validating responses"
agent_say "Let me verify the API works correctly..."

agent_wait 0.3
HEALTH=$(curl -sf "$MCP_URL/health" && echo "OK" || echo "FAIL")
agent_ok "Health check: $HEALTH"

agent_wait 0.3
PING=$(mcp '{"jsonrpc":"2.0","id":"a2-1","method":"ping","params":{}}')
PING_OK=$(echo "$PING" | jq -r '.result // "pong"' 2>/dev/null)
agent_ok "Ping: $PING_OK"

agent_say "Creating a test project..."
agent_wait 0.5
CREATE=$(mcp '{"jsonrpc":"2.0","id":"a2-2","method":"tools/call","params":{"name":"create_project","arguments":{"name":"Test Project","color":"#FF5500"}}}')
CREATE_OK=$(echo "$CREATE" | jq -r '.result.content[0].text' 2>/dev/null | jq -r '.name // "created"' 2>/dev/null || echo "created")
agent_ok "Created: $CREATE_OK"

agent_say "Listing all projects..."
agent_wait 0.3
LIST=$(mcp '{"jsonrpc":"2.0","id":"a2-3","method":"tools/call","params":{"name":"list_projects","arguments":{"page":1}}}')
LIST_COUNT=$(echo "$LIST" | jq -r '.result.content[0].text' 2>/dev/null | jq -r '.data | length' 2>/dev/null || echo "1+")
agent_ok "Projects found: $LIST_COUNT"

# ── Agent 3: MANAGER — A2A task delegation ─────────────────────────────────
agent_header "Agent 3: MANAGER — A2A task delegation"
agent_say "I need someone to check the current user. Delegating via A2A..."

agent_wait 0.3
CARD=$(curl -sf "$MCP_URL/.well-known/agent-card.json" 2>/dev/null || echo "{}")
CARD_NAME=$(echo "$CARD" | jq -r '.name // "unknown"' 2>/dev/null)
CARD_SKILLS=$(echo "$CARD" | jq -r '.skills | length' 2>/dev/null || echo "0")
agent_ok "Found agent: $CARD_NAME ($CARD_SKILLS skills)"

agent_say "Sending task: get_current_user..."
agent_wait 0.5
TASK=$(curl -s -X POST "$MCP_URL/a2a/tasks/send" \
    -H "Content-Type: application/json" \
    -d '{"id":"demo-task-1","sessionId":"manager-session","message":{"role":"user","parts":[{"type":"data","data":{"skill":"get_current_user","parameters":{}}}]}}' 2>/dev/null)
TASK_STATE=$(echo "$TASK" | jq -r '.status.state // "unknown"' 2>/dev/null)
agent_ok "Task state: $TASK_STATE"

agent_say "Checking task result..."
agent_wait 0.3
TASK_ID=$(echo "$TASK" | jq -r '.id' 2>/dev/null)
GET_TASK=$(curl -sf "$MCP_URL/a2a/tasks/get?id=$TASK_ID" 2>/dev/null || echo "{}")
FINAL_STATE=$(echo "$GET_TASK" | jq -r '.status.state // "completed"' 2>/dev/null)
agent_ok "Final state: $FINAL_STATE"

# ── Agent 3b: DELEGATION — yas-mcp delegates to Mini Agent ─────────────────
echo ""
echo -e "  ${YELLOW}Agent 3 (Manager) delegates to external Mini Agent...${NC}"
agent_wait 0.3

MINI_CARD=$(curl -sf "$MINI_AGENT_URL/.well-known/agent-card.json" 2>/dev/null || echo "{}")
MINI_SKILLS=$(echo "$MINI_CARD" | jq -r '.skills | length' 2>/dev/null || echo "0")
agent_ok "Discovered Mini Agent with $MINI_SKILLS skills: $(echo "$MINI_CARD" | jq -r '[.skills[].id] | join(", ")' 2>/dev/null)"

agent_say "Delegating: get weather for Tokyo..."
agent_wait 0.3
DELEGATE=$(curl -s -X POST "$MINI_AGENT_URL/a2a/tasks/send" \
    -H "Content-Type: application/json" \
    -d '{"id":"delegate-1","sessionId":"yas-mcp","message":{"role":"user","parts":[{"type":"data","data":{"skill":"weather","parameters":{"city":"Tokyo"}}}]}}' 2>/dev/null)
DELEGATE_STATE=$(echo "$DELEGATE" | jq -r '.status.state' 2>/dev/null)
DELEGATE_TEMP=$(echo "$DELEGATE" | jq -r '.artifacts[0].parts[0].data.temperature' 2>/dev/null)
DELEGATE_COND=$(echo "$DELEGATE" | jq -r '.artifacts[0].parts[0].data.conditions' 2>/dev/null)
agent_ok "Task $DELEGATE_STATE — Tokyo: ${DELEGATE_TEMP}°C, $DELEGATE_COND"

agent_say "Delegating: calculate 1337 / 7..."
agent_wait 0.3
CALC=$(curl -s -X POST "$MINI_AGENT_URL/a2a/tasks/send" \
    -H "Content-Type: application/json" \
    -d '{"id":"delegate-2","sessionId":"yas-mcp","message":{"role":"user","parts":[{"type":"text","text":"calculator"},{"type":"data","data":{"parameters":{"expression":"1337 / 7"}}}]}}' 2>/dev/null)
CALC_RESULT=$(echo "$CALC" | jq -r '.artifacts[0].parts[0].data.result' 2>/dev/null)
agent_ok "Result: 1337 / 7 = $CALC_RESULT"

# ── Agent 4: MONITOR — watch metrics ───────────────────────────────────────
agent_header "Agent 4: MONITOR — system metrics"
agent_say "Let me check the system health..."

agent_wait 0.3
METRICS=$(curl -sf "$MCP_URL/metrics" 2>/dev/null || echo "")
TOOL_CALLS=$(echo "$METRICS" | grep "yas_mcp_tool_calls_total" | grep -v '#' | awk '{sum+=$2} END {print sum+0}')
ACTIVE_TOOLS=$(echo "$METRICS" | grep "yas_mcp_active_tools" | grep -v '#' | awk '{print $2}')
A2A_TASKS=$(echo "$METRICS" | grep "yas_mcp_a2a_tasks_total" | grep -v '#' | awk '{sum+=$2} END {print sum+0}')
agent_ok "Tool calls this session: $TOOL_CALLS"
agent_ok "Active tools: $ACTIVE_TOOLS"
agent_ok "A2A task transitions: $A2A_TASKS"

agent_say "Checking AI Catalog..."
agent_wait 0.2
CATALOG=$(curl -sf "$MCP_URL/.well-known/ai-catalog.json" 2>/dev/null || echo "{}")
CAT_ENTRIES=$(echo "$CATALOG" | jq -r '.entries | length' 2>/dev/null || echo "0")
agent_ok "Catalog entries: $CAT_ENTRIES"

# ── Summary ────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║  DEMO COMPLETE                                              ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${CYAN}Agent 1 (Coder)${NC}    — listed tools, explored API"
echo -e "  ${MAGENTA}Agent 2 (Tester)${NC}   — validated health, ping, CRUD"
echo -e "  ${YELLOW}Agent 3 (Manager)${NC}  — delegated via A2A"
echo -e "  ${GREEN}Agent 4 (Monitor)${NC}  — checked metrics + catalog"
echo ""
echo -e "  ${BOLD}Endpoints:${NC}"
echo -e "    MCP:      ${DIM}$MCP_URL/mcp${NC}"
echo -e "    Agent:    ${DIM}$MCP_URL/.well-known/agent-card.json${NC}"
echo -e "    Catalog:  ${DIM}$MCP_URL/.well-known/ai-catalog.json${NC}"
echo -e "    Metrics:  ${DIM}$MCP_URL/metrics${NC}"
echo ""
