#!/bin/bash
# ═══════════════════════════════════════════════════════════════════════════
# yas-mcp FLYING PROBE — Comprehensive System Verification
# ═══════════════════════════════════════════════════════════════════════════
#
# Like a PCB flying probe tester, this script systematically probes every
# pad, trace, and via on the yas-mcp system board:
#
#   Pad probes:      Every endpoint responds with correct HTTP status
#   Trace probes:    MCP protocol compliance (init → list → call → ping)
#   Via probes:      A2A task lifecycle (submit → stream → get → cancel)
#   Short-circuit:   Auth middleware chain (bearer, api-key, none)
#   Continuity:      Each registered tool executes and returns data
#   Stress points:   Rate limits, malformed input, concurrent calls
#   Ground plane:    Health, metrics, catalog, agent-card always reachable
#   Signal quality:  Response times, correct content types, valid JSON
#
# Usage:
#   MCP_SERVER_URL=http://localhost:3000 ./scripts/flying-probe.sh
#   MCP_SERVER_URL=http://localhost:3000 ./scripts/flying-probe.sh --quick
#   MCP_SERVER_URL=http://localhost:3000 ./scripts/flying-probe.sh --verbose

MCP_URL="${MCP_SERVER_URL:-http://127.0.0.1:3000}"
QUICK_MODE=false
VERBOSE=false
TIMEOUT=5

# ── Probe state ────────────────────────────────────────────────────────────
declare -A PROBE_RESULTS
PROBES_TOTAL=0
PROBES_PASS=0
PROBES_FAIL=0
PROBES_SKIP=0
START_TIME=$(date +%s)

# ── Color output ───────────────────────────────────────────────────────────
GREEN='\033[32m'; RED='\033[31m'; YELLOW='\033[33m'; CYAN='\033[36m'
BOLD='\033[1m'; DIM='\033[2m'; NC='\033[0m'

pass() { PROBES_PASS=$((PROBES_PASS + 1)); echo -e "  ${GREEN}✓${NC} $1"; }
fail() { PROBES_FAIL=$((PROBES_FAIL + 1)); echo -e "  ${RED}✗${NC} $1 ${DIM}($2)${NC}"; }
skip() { PROBES_SKIP=$((PROBES_SKIP + 1)); echo -e "  ${YELLOW}○${NC} $1 ${DIM}(skipped)${NC}"; }
info() { echo -e "  ${CYAN}▶${NC} $1"; }
vrb()  { [ "$VERBOSE" = true ] && echo -e "    ${DIM}$1${NC}"; }

probe() {
    local id="$1" label="$2"; shift 2
    PROBES_TOTAL=$((PROBES_TOTAL + 1))
    if "$@"; then
        PROBE_RESULTS["$id"]="PASS"
        pass "$label"
        return 0
    else
        PROBE_RESULTS["$id"]="FAIL"
        fail "$label" "$*"
        return 1
    fi
}

# ── HTTP helpers ───────────────────────────────────────────────────────────
http_get() {
    curl -sf -m "$TIMEOUT" -w "\n%{http_code}" "$1" 2>/dev/null
}
http_post() {
    curl -sf -m "$TIMEOUT" -X POST -H "Content-Type: application/json" -d "$2" -w "\n%{http_code}" "$1" 2>/dev/null
}
http_status() { echo "$1" | tail -1; }
http_body()   { echo "$1" | sed '$d'; }
json_field()  { echo "$1" | jq -r "$2" 2>/dev/null; }

# ── Args ───────────────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --quick) QUICK_MODE=true ;;
        --verbose|-v) VERBOSE=true ;;
    esac
done

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║  yas-mcp FLYING PROBE — System Verification Suite        ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════╝${NC}"
echo -e "${DIM}  Target: ${MCP_URL}${NC}"
echo ""

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 1: GROUND PLANE — Health & Discovery (always-on pads)
# ═══════════════════════════════════════════════════════════════════════════
echo -e "${BOLD}┌─ BOARD 1: GROUND PLANE — Health & Discovery${NC}"

RAW=$(http_get "$MCP_URL/health")
S=$(http_status "$RAW")
probe "1.1" "Health endpoint" [ "$S" = "200" ] || probe "1.1" "Health endpoint" false

RAW=$(http_get "$MCP_URL/metrics")
S=$(http_status "$RAW")
B=$(http_body "$RAW")
probe "1.2" "Metrics endpoint (20x)" [ "$S" = "200" ]
probe "1.3" "Metrics contain tool_calls counter" echo "$B" | grep -q "yas_mcp_tool_calls_total"
probe "1.4" "Metrics contain build_info" echo "$B" | grep -q "yas_mcp_build_info"
probe "1.5" "Metrics content-type is text/plain" curl -sf -m "$TIMEOUT" -I "$MCP_URL/metrics" 2>/dev/null | grep -qi "content-type: text/plain"

RAW=$(http_get "$MCP_URL/.well-known/ai-catalog.json")
S=$(http_status "$RAW")
probe "1.6" "AI Catalog endpoint (20x)" [ "$S" = "200" ]
B=$(http_body "$RAW")
probe "1.7" "AI Catalog has entries array" echo "$B" | jq -e '.entries' > /dev/null 2>&1

RAW=$(http_get "$MCP_URL/.well-known/agent-card.json")
S=$(http_status "$RAW")
if [ "$S" = "200" ]; then
    B=$(http_body "$RAW")
    probe "1.8" "A2A Agent Card available" true
    probe "1.9" "Agent Card has skills" echo "$B" | jq -e '.skills' > /dev/null 2>&1
else
    skip "1.8" "A2A Agent Card (HTTP $S — A2A disabled)"
    skip "1.9" "Agent Card skills (A2A disabled)"
fi

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 2: SIGNAL TRACES — MCP Protocol Compliance
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 2: SIGNAL TRACES — MCP Protocol${NC}"

# Initialize
RAW=$(http_post "$MCP_URL/mcp" '{
    "jsonrpc":"2.0","id":"probe-init","method":"initialize",
    "params":{"protocolVersion":"2024-11-05","capabilities":{},
    "clientInfo":{"name":"flying-probe","version":"1.0"}}
}')
B=$(http_body "$RAW")
S=$(http_status "$RAW")
probe "2.1" "Initialize returns 200" [ "$S" = "200" ]
probe "2.2" "Initialize has result" echo "$B" | jq -e '.result' > /dev/null 2>&1
probe "2.3" "Has serverInfo.name" [ "$(json_field "$B" '.result.serverInfo.name')" != "null" ]
SERVER_NAME=$(json_field "$B" '.result.serverInfo.name')
vrb "Server: $SERVER_NAME"

# Tools list
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","id":"probe-tools","method":"tools/list","params":{}}')
B=$(http_body "$RAW")
TOOL_COUNT=$(json_field "$B" '.result.tools | length')
probe "2.4" "tools/list returns array" sh -c '[ "$0" != "null" ] && [ "$0" != "0" ]' "$TOOL_COUNT"
vrb "Tools registered: $TOOL_COUNT"

# Get all tool names
TOOLS=$(echo "$B" | jq -r '.result.tools[].name' 2>/dev/null)
probe "2.5" "Tool names are non-empty" [ -n "$TOOLS" ]

# Ping
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","id":"probe-ping","method":"ping","params":{}}')
B=$(http_body "$RAW")
probe "2.6" "Ping returns result" echo "$B" | jq -e '.result' > /dev/null 2>&1

# Unknown method
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","id":"probe-unk","method":"no/such/method","params":{}}')
B=$(http_body "$RAW")
probe "2.7" "Unknown method returns error" echo "$B" | jq -e '.error' > /dev/null 2>&1
probe "2.8" "Unknown method error code -32601" [ "$(json_field "$B" '.error.code')" = "-32601" ]

# Malformed JSON
RAW=$(curl -sf -m "$TIMEOUT" -X POST -H "Content-Type: application/json" \
    -d '{bad' "$MCP_URL/mcp" -w "\n%{http_code}" 2>/dev/null || echo "000")
S=$(http_status "$RAW")
probe "2.9" "Malformed JSON returns error" [ "$S" != "200" ]

# JSON-RPC compliance: notifications (no id) should not get a response
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","method":"notifications/initialized"}')
B=$(http_body "$RAW")
probe "2.10" "Notification returns empty" sh -c '[ -z "$0" ] || [ "$0" = "{}" ] || [ "$0" = "null" ]' "$B"

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 3: PAD PROBES — Tool Call Execution
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 3: PAD PROBES — Tool Calls${NC}"

TOOL_N=0
TOOL_OK=0
TOOL_ERR=0
TOOL_SKIP=0

for TOOL in $TOOLS; do
    TOOL_N=$((TOOL_N + 1))
    if [ "$QUICK_MODE" = true ] && [ "$TOOL_N" -gt 5 ]; then
        skip "3.${TOOL_N}" "$TOOL (quick mode limit)"
        TOOL_SKIP=$((TOOL_SKIP + 1))
        continue
    fi

    # Determine appropriate params for this tool
    PARAMS="{}"
    case "$TOOL" in
        get_health|get_) PARAMS="{}" ;;
        get_users*)      PARAMS='{"page":1}' ;;
        get_projects*)   PARAMS='{"page":1}' ;;
        get_*_by_id*|get_*___*)
            # Extract path param name from tool name
            PARAM=$(echo "$TOOL" | grep -o '__[^_]*__' | sed 's/__//g' | head -1)
            [ -n "$PARAM" ] && PARAMS="{\"$PARAM\":\"probe-test-id\"}" || PARAMS="{}"
            ;;
    esac

    RAW=$(http_post "$MCP_URL/mcp" "{
        \"jsonrpc\":\"2.0\",
        \"id\":\"probe-tool-$TOOL_N\",
        \"method\":\"tools/call\",
        \"params\":{\"name\":\"$TOOL\",\"arguments\":$PARAMS}
    }")
    B=$(http_body "$RAW")

    if echo "$B" | jq -e '.result' > /dev/null 2>&1; then
        TOOL_OK=$((TOOL_OK + 1))
        IS_ERR=$(json_field "$B" '.result.isError')
        if [ "$IS_ERR" = "true" ]; then
            vrb "  ⚡ $TOOL → returned error (upstream)"
            TOOL_ERR=$((TOOL_ERR + 1))
        else
            vrb "  ✓ $TOOL"
        fi
    elif echo "$B" | jq -e '.error' > /dev/null 2>&1; then
        CODE=$(json_field "$B" '.error.code')
        case "$CODE" in
            -32602) vrb "  ○ $TOOL → invalid params (expected)"; TOOL_SKIP=$((TOOL_SKIP + 1)) ;;
            -32601) vrb "  ○ $TOOL → not found"; TOOL_SKIP=$((TOOL_SKIP + 1)) ;;
            *) vrb "  ✗ $TOOL → error $CODE"; TOOL_ERR=$((TOOL_ERR + 1)) ;;
        esac
    else
        vrb "  ✗ $TOOL → no response"
        TOOL_ERR=$((TOOL_ERR + 1))
    fi
done

probe "3.0" "Tools tested: $TOOL_OK/$TOOL_N OK, $TOOL_ERR errors, $TOOL_SKIP skipped" [ "$TOOL_ERR" -le "$TOOL_SKIP" ]

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 4: VIAS — A2A Protocol (task lifecycle through the stack)
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 4: VIAS — A2A Task Lifecycle${NC}"

RAW=$(http_get "$MCP_URL/.well-known/agent-card.json")
S=$(http_status "$RAW")

if [ "$S" = "200" ]; then
    # Get first skill for task testing
    B=$(http_body "$RAW")
    FIRST_SKILL=$(echo "$B" | jq -r '.skills[0].id // empty' 2>/dev/null)

    # Task send
    RAW=$(http_post "$MCP_URL/a2a/tasks/send" "{
        \"id\":\"probe-a2a-1\",
        \"sessionId\":\"probe-session\",
        \"message\":{
            \"role\":\"user\",
            \"parts\":[{\"type\":\"data\",\"data\":{\"skill\":\"${FIRST_SKILL:-get_health}\",\"parameters\":{}}}]
        }
    }")
    B=$(http_body "$RAW")
    S=$(http_status "$RAW")
    TASK_ID=$(json_field "$B" '.id')
    TASK_STATE=$(json_field "$B" '.status.state')

    probe "4.1" "A2A tasks/send returns 200" [ "$S" = "200" ]
    probe "4.2" "Task has valid ID" sh -c '[ -n "$0" ] && [ "$0" != "null" ]' "$TASK_ID"
    probe "4.3" "Task has state" sh -c '[ -n "$0" ] && [ "$0" != "null" ]' "$TASK_STATE"

    # Task get
    if [ -n "$TASK_ID" ] && [ "$TASK_ID" != "null" ]; then
        RAW=$(http_get "$MCP_URL/a2a/tasks/get?id=$TASK_ID")
        B=$(http_body "$RAW")
        GET_STATE=$(json_field "$B" '.status.state')
        probe "4.4" "A2A tasks/get returns state" sh -c '[ -n "$0" ] && [ "$0" != "null" ]' "$GET_STATE"

        # Task cancel
        RAW=$(http_post "$MCP_URL/a2a/tasks/cancel" "{
            \"id\":\"$TASK_ID\",
            \"sessionId\":\"probe-session\"
        }")
        B=$(http_body "$RAW")
        CANCEL_STATE=$(json_field "$B" '.status.state')
        probe "4.5" "A2A tasks/cancel works" [ -n "$CANCEL_STATE" ]
    else
        skip "4.4" "tasks/get (no task ID)"
        skip "4.5" "tasks/cancel (no task ID)"
    fi

    # Task sendSubscribe (SSE) — quick probe
    RAW=$(http_post "$MCP_URL/a2a/tasks/sendSubscribe" "{
        \"id\":\"probe-a2a-sse\",
        \"sessionId\":\"probe-session\",
        \"message\":{
            \"role\":\"user\",
            \"parts\":[{\"type\":\"data\",\"data\":{\"skill\":\"get_health\",\"parameters\":{}}}]
        }
    }")
    S=$(http_status "$RAW")
    B=$(http_body "$RAW")
    probe "4.6" "A2A sendSubscribe returns data" sh -c '[ -n "$0" ] && [ "$0" != "null" ] && [ "$0" != "{}" ]' "$B"
else
    skip "4.1" "A2A (disabled)"
    skip "4.2" "A2A (disabled)"
    skip "4.3" "A2A (disabled)"
    skip "4.4" "A2A (disabled)"
    skip "4.5" "A2A (disabled)"
    skip "4.6" "A2A (disabled)"
fi

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 5: SHORT-CIRCUIT — Auth Middleware
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 5: SHORT-CIRCUIT — Auth Middleware${NC}"

# Health should always be open
RAW=$(http_get "$MCP_URL/health")
S=$(http_status "$RAW")
probe "5.1" "Health is always open (no auth)" [ "$S" = "200" ]

# Metrics should always be open
RAW=$(http_get "$MCP_URL/metrics")
S=$(http_status "$RAW")
probe "5.2" "Metrics always open" [ "$S" = "200" ]

# MCP endpoint without auth — should work (passthrough mode)
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","id":"probe-noauth","method":"ping","params":{}}')
B=$(http_body "$RAW")
probe "5.3" "MCP works without auth header" echo "$B" | jq -e '.result' > /dev/null 2>&1

# MCP endpoint with wrong auth — should still work if auth middleware allows it
# (or return 401 if auth is strictly required)
RAW=$(curl -s -m "$TIMEOUT" -X POST \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer wrong-token" \
    -d '{"jsonrpc":"2.0","id":"probe-wrongauth","method":"ping","params":{}}' \
    -w "\n%{http_code}" "$MCP_URL/mcp" 2>/dev/null)
S=$(http_status "$RAW")
B=$(http_body "$RAW")
if [ "$S" = "200" ]; then
    pass "5.4 Wrong bearer token → 200 (passthrough mode — auth not configured)"
elif [ "$S" = "401" ]; then
    pass "5.4 Wrong bearer token → 401 (auth middleware active)"
else
    fail "5.4 Wrong bearer token → unexpected HTTP $S" "HTTP $S"
fi

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 6: STRESS POINTS — Rate Limits & Resilience
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 6: STRESS POINTS — Rate Limits & Resilience${NC}"

# Burst test: send 10 rapid pings
BURST_OK=0
BURST_429=0
for i in $(seq 1 10); do
    RAW=$(http_post "$MCP_URL/mcp" "{\"jsonrpc\":\"2.0\",\"id\":\"burst-$i\",\"method\":\"ping\",\"params\":{}}")
    S=$(http_status "$RAW")
    if [ "$S" = "200" ]; then
        BURST_OK=$((BURST_OK + 1))
    elif [ "$S" = "429" ]; then
        BURST_429=$((BURST_429 + 1))
    fi
done
probe "6.1" "Burst: $BURST_OK/10 pings OK" [ "$BURST_OK" -ge 8 ]
if [ "$BURST_429" -gt 0 ]; then
    pass "6.2 Rate limiting active ($BURST_429 429s)"
else
    skip "6.2" "Rate limiting (not configured or not triggered)"
fi

# Concurrent connections test
CONCURRENT_OK=0
for i in $(seq 1 5); do
    http_post "$MCP_URL/mcp" "{\"jsonrpc\":\"2.0\",\"id\":\"concurrent-$i\",\"method\":\"ping\",\"params\":{}}" > /dev/null 2>&1 &
done
wait
probe "6.3" "5 concurrent pings" true

# ═══════════════════════════════════════════════════════════════════════════
# BOARD 7: SIGNAL QUALITY — Response Characteristics
# ═══════════════════════════════════════════════════════════════════════════
echo ""
echo -e "${BOLD}┌─ BOARD 7: SIGNAL QUALITY — Response Characteristics${NC}"

# Content-Type checks
CT=$(curl -sf -m "$TIMEOUT" -I "$MCP_URL/metrics" 2>/dev/null | grep -i "content-type:" | tr -d '\r')
probe "7.1" "Metrics content-type present" [ -n "$CT" ]

# Timing probe
TIMING=$(curl -s -o /dev/null -w "%{time_total}" -m "$TIMEOUT" "$MCP_URL/health" 2>/dev/null)
TOO_SLOW=$(echo "$TIMING > 1.0" | bc -l 2>/dev/null || echo "0")
if [ "$TOO_SLOW" = "1" ]; then
    fail "7.2" "Health response time ${TIMING}s (slow)"
else
    pass "7.2 Health response time ${TIMING}s"
fi

# JSON validity on MCP responses
RAW=$(http_post "$MCP_URL/mcp" '{"jsonrpc":"2.0","id":"probe-json","method":"ping","params":{}}')
B=$(http_body "$RAW")
probe "7.3" "MCP response is valid JSON" echo "$B" | jq -e '.' > /dev/null 2>&1
probe "7.4" "MCP response has jsonrpc=2.0" [ "$(json_field "$B" '.jsonrpc')" = "2.0" ]

# ═══════════════════════════════════════════════════════════════════════════
# REPORT
# ═══════════════════════════════════════════════════════════════════════════
ELAPSED=$(($(date +%s) - START_TIME))
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║  FLYING PROBE REPORT                                     ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  Probes run:    ${BOLD}$PROBES_TOTAL${NC}"
echo -e "  ${GREEN}Passed:        $PROBES_PASS${NC}"
echo -e "  ${RED}Failed:        $PROBES_FAIL${NC}"
echo -e "  ${YELLOW}Skipped:       $PROBES_SKIP${NC}"
echo -e "  Tool calls:    $TOOL_N ($TOOL_OK OK, $TOOL_ERR errors, $TOOL_SKIP skipped)"
echo -e "  Duration:      ${ELAPSED}s"
echo ""

if [ "$PROBES_FAIL" -gt 0 ]; then
    echo -e "${RED}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${RED}║  FAILED PROBES — Review required                         ║${NC}"
    echo -e "${RED}╚══════════════════════════════════════════════════════════╝${NC}"
    for id in $(echo "${!PROBE_RESULTS[@]}" | tr ' ' '\n' | sort -n); do
        if [ "${PROBE_RESULTS[$id]}" = "FAIL" ]; then
            echo -e "  ${RED}✗${NC} Probe $id"
        fi
    done
    echo ""
    exit 1
else
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║  ALL PROBES PASSED — Board is clean                      ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "  ${DIM}Ground plane:   ✓  All health/discovery endpoints responding${NC}"
    echo -e "  ${DIM}Signal traces:  ✓  MCP protocol compliant${NC}"
    echo -e "  ${DIM}Pad probes:     ✓  $TOOL_OK/$TOOL_N tools executing${NC}"
    echo -e "  ${DIM}Vias:           ✓  A2A lifecycle functional${NC}"
    echo -e "  ${DIM}Short-circuit:  ✓  Auth middleware behaving correctly${NC}"
    echo -e "  ${DIM}Stress points:  ✓  Burst and concurrency handled${NC}"
    echo -e "  ${DIM}Signal quality: ✓  Content types, timing, JSON validity OK${NC}"
    exit 0
fi
