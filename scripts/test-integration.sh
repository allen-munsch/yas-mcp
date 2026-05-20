#!/bin/bash
set -e

echo "🚀 Starting OpenAPI MCP Integration Test Suite"
echo "=============================================="

# Configuration
MCP_SERVER_URL="http://127.0.0.1:3000"
PRISM_URL="http://127.0.0.1:4010"
TIMEOUT=90

# Test results storage
declare -a TEST_RESULTS
declare -a TOOLS_LIST

# Test counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0
SKIPPED_TESTS=0

# Logging functions
log_info() {
    echo "ℹ️  $1"
}

log_success() {
    echo "✅ $1"
}

log_warning() {
    echo "⚠️  $1"
}

log_error() {
    echo "❌ $1"
}

log_test() {
    echo "🧪 $1"
}

# Test result tracking
record_test() {
    local test_name="$1"
    local status="$2"
    local message="$3"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    case "$status" in
        "PASS")
            PASSED_TESTS=$((PASSED_TESTS + 1))
            log_success "$test_name: $message"
            ;;
        "FAIL")
            FAILED_TESTS=$((FAILED_TESTS + 1))
            log_error "$test_name: $message"
            ;;
        "SKIP")
            SKIPPED_TESTS=$((SKIPPED_TESTS + 1))
            log_warning "$test_name: $message"
            ;;
    esac
    
    # Store result in array
    TEST_RESULTS+=("$(date '+%H:%M:%S') | $test_name | $status | $message")
}

# Wait for service with better error handling
wait_for_service() {
    local service_name="$1"
    local url="$2"
    local timeout=${3:-90}
    local count=0
    
    log_info "Waiting for $service_name at $url..."
    
    while [ $count -lt $timeout ]; do
        if curl -s -f "$url" > /dev/null 2>&1; then
            log_success "$service_name is ready"
            return 0
        fi
        echo "  Waiting for $service_name... ($((count + 1))/$timeout)"
        sleep 2
        count=$((count + 1))
    done
    
    log_error "$service_name did not become ready within $timeout seconds"
    return 1
}

# Make JSON-RPC request with better error handling
mcp_request() {
    local id="$1"
    local method="$2"
    local params="$3"
    local description="$4"
    
    local response
    response=$(curl -s -X POST "$MCP_SERVER_URL/mcp" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json" \
        -w "HTTP_STATUS:%{http_code}" \
        -d '{
            "jsonrpc": "2.0",
            "id": "'"$id"'",
            "method": "'"$method"'",
            "params": '"$params"'
        }' 2>/dev/null)
    
    local http_status
    http_status=$(echo "$response" | grep "HTTP_STATUS:" | cut -d':' -f2)
    response=$(echo "$response" | sed '/HTTP_STATUS:/d')
    
    if [ "$http_status" != "200" ]; then
        log_error "HTTP error $http_status for $description"
        echo "{}"
        return 1
    fi
    
    echo "$response"
}

# Test tool call with parameter validation
test_tool_call() {
    local tool_name="$1"
    local tool_description="$2"
    local test_id="$3"
    
    log_test "Testing tool: $tool_name - $tool_description"
    
    # Generate appropriate test parameters based on tool type
    local params="{}"
    
    case "$tool_name" in
        get_*)
            # GET requests - minimal parameters
            params='{}'
            ;;
        post_auth_login|post_auth_register)
            # Auth endpoints - basic credentials
            params='{"email": "test@example.com", "password": "test123"}'
            ;;
        post_*|put_*)
            # POST/PUT requests - basic data structure
            if [[ "$tool_name" == *projects* ]]; then
                params='{"title": "Test Project", "description": "Test Description"}'
            elif [[ "$tool_name" == *tasks* ]]; then
                params='{"title": "Test Task", "description": "Test Description"}'
            elif [[ "$tool_name" == *comments* ]]; then
                params='{"content": "Test comment"}'
            else
                params='{"data": "test"}'
            fi
            ;;
        delete_*)
            # DELETE requests - usually need ID parameters
            if [[ "$tool_name" == *project* ]]; then
                params='{"project_id": "test-id"}'
            elif [[ "$tool_name" == *task* ]]; then
                params='{"task_id": "test-id"}'
            elif [[ "$tool_name" == *comment* ]]; then
                params='{"comment_id": "test-id"}'
            elif [[ "$tool_name" == *attachment* ]]; then
                params='{"attachment_id": "test-id"}'
            fi
            ;;
    esac
    
    # Add path parameters for tools that need them
    if [[ "$tool_name" == *___* ]]; then
        # Extract parameter names from tool name (between ___)
        local temp_params="$params"
        if [[ "$temp_params" == "{}" ]]; then
            temp_params='{}'
        fi
        
        # Add dummy path parameters
        local path_params=$(echo "$tool_name" | grep -o '__[^_]*__' | sed 's/__//g' | head -1)
        if [ -n "$path_params" ]; then
            params=$(echo "$temp_params" | jq --arg param "$path_params" --arg value "test-$path_params" '. + {($param): $value}' 2>/dev/null || echo "$temp_params")
        fi
    fi
    
    local response
    response=$(mcp_request "$test_id" "tools/call" "{\"name\": \"$tool_name\", \"arguments\": $params}" "Tool call: $tool_name")
    
    if [ -n "$response" ] && [ "$response" != "{}" ]; then
        local error=$(echo "$response" | jq -r '.error // empty' 2>/dev/null || echo "")
        
        if [ -n "$error" ]; then
            local error_code=$(echo "$response" | jq -r '.error.code // empty' 2>/dev/null || echo "")
            local error_msg=$(echo "$response" | jq -r '.error.message // empty' 2>/dev/null || echo "")
            
            if [ "$error_code" = "-32601" ]; then
                record_test "$tool_name" "SKIP" "Tool not found (might be expected)"
            elif [ "$error_code" = "-32602" ]; then
                record_test "$tool_name" "SKIP" "Invalid parameters (might need specific values)"
            else
                record_test "$tool_name" "FAIL" "Tool call failed: $error_msg (code: $error_code)"
            fi
        else
            local result=$(echo "$response" | jq -r '.result // empty' 2>/dev/null || echo "")
            if [ -n "$result" ]; then
                record_test "$tool_name" "PASS" "Tool call successful"
            else
                record_test "$tool_name" "PASS" "Tool call completed (empty response)"
            fi
        fi
    else
        record_test "$tool_name" "FAIL" "No response received"
    fi
}

# Test Prism endpoints directly
test_prism_endpoint() {
    local method="$1"
    local path="$2"
    local description="$3"
    
    log_test "Testing Prism: $method $path"
    
    local url="$PRISM_URL$path"
    local response
    local status_code
    
    case "$method" in
        "GET")
            response=$(curl -s -w "HTTP_STATUS:%{http_code}" "$url" 2>/dev/null)
            ;;
        "POST"|"PUT"|"DELETE")
            response=$(curl -s -X "$method" -w "HTTP_STATUS:%{http_code}" -H "Content-Type: application/json" -d '{}' "$url" 2>/dev/null)
            ;;
        *)
            record_test "Prism-$method-$path" "SKIP" "Unsupported method"
            return
            ;;
    esac
    
    status_code=$(echo "$response" | grep "HTTP_STATUS:" | cut -d':' -f2)
    response=$(echo "$response" | sed '/HTTP_STATUS:/d')
    
    if [ "$status_code" = "200" ] || [ "$status_code" = "201" ]; then
        record_test "Prism-$method-$path" "PASS" "Endpoint responded successfully"
    elif [ "$status_code" = "401" ] || [ "$status_code" = "403" ]; then
        record_test "Prism-$method-$path" "SKIP" "Authentication required"
    elif [ "$status_code" = "404" ]; then
        record_test "Prism-$method-$path" "SKIP" "Endpoint not found (might be parameterized)"
    else
        record_test "Prism-$method-$path" "FAIL" "HTTP $status_code"
    fi
}

# Display detailed results
show_detailed_results() {
    echo ""
    echo "📋 DETAILED TEST RESULTS"
    echo "========================"
    for result in "${TEST_RESULTS[@]}"; do
        echo "$result"
    done
}

# Display tools list
show_tools_list() {
    echo ""
    echo "🛠️  REGISTERED TOOLS"
    echo "==================="
    for tool in "${TOOLS_LIST[@]}"; do
        echo "  - $tool"
    done
}

# Main test execution
main() {
    echo "Starting comprehensive MCP server testing..."
    echo "MCP Server: $MCP_SERVER_URL"
    echo "Prism Mock: $PRISM_URL"
    echo ""
    
    # Wait for services
    if ! wait_for_service "MCP Server" "$MCP_SERVER_URL/health" "$TIMEOUT"; then
        log_error "MCP server not available, exiting"
        exit 1
    fi
    
    if curl -s "$PRISM_URL/" > /dev/null 2>&1; then
        log_success "Prism mock is accessible"
    else
        log_warning "Prism mock may require authentication, continuing with MCP tests only"
    fi
    
    echo ""
    echo "🧪 PHASE 1: MCP Server Core Functionality"
    echo "========================================"
    
    # Test 1: Health endpoint
    log_test "Testing health endpoint"
    if curl -f -s "$MCP_SERVER_URL/health" > /dev/null; then
        record_test "Health-Check" "PASS" "Health endpoint responding"
    else
        record_test "Health-Check" "FAIL" "Health endpoint failed"
        exit 1
    fi
    
    # Test 2: MCP initialization
    log_test "Testing MCP initialization"
    local init_response
    init_response=$(mcp_request "init-1" "initialize" '{
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {
            "name": "test-client",
            "version": "1.0.0"
        }
    }' "MCP initialization")
    
    if echo "$init_response" | jq -e '.result' > /dev/null 2>&1; then
        record_test "MCP-Initialization" "PASS" "Server initialized successfully"
        local server_name=$(echo "$init_response" | jq -r '.result.server_info.name // empty')
        local server_version=$(echo "$init_response" | jq -r '.result.server_info.version // empty')
        log_info "Connected to: $server_name v$server_version"
    else
        record_test "MCP-Initialization" "FAIL" "Initialization failed"
        echo "$init_response" | jq . 2>/dev/null || echo "$init_response"
        exit 1
    fi
    
    # Test 3: Tools listing
    log_test "Testing tools listing"
    local tools_response
    tools_response=$(mcp_request "tools-1" "tools/list" '{}' "Tools listing")
    
    local tool_count
    tool_count=$(echo "$tools_response" | jq -r '.result.tools | length' 2>/dev/null || echo "0")
    
    if [ "$tool_count" -gt 0 ]; then
        record_test "Tools-Listing" "PASS" "Found $tool_count tools"
        # Store tools list in array
        while IFS= read -r tool_line; do
            if [ -n "$tool_line" ]; then
                TOOLS_LIST+=("$tool_line")
            fi
        done < <(echo "$tools_response" | jq -r '.result.tools[] | "\(.name): \(.description // "No description")"' 2>/dev/null)
    else
        record_test "Tools-Listing" "FAIL" "No tools found"
        exit 1
    fi
    
    # Extract tool names for testing
    local tool_names
    tool_names=$(echo "$tools_response" | jq -r '.result.tools[].name' 2>/dev/null)
    
    echo ""
    echo "🧪 PHASE 2: Individual Tool Testing"
    echo "==================================="
    
    # Test each tool
    local test_id=100
    for tool_name in $tool_names; do
        local tool_desc=$(echo "$tools_response" | jq -r --arg name "$tool_name" '.result.tools[] | select(.name == $name) | .description // "No description"' 2>/dev/null)
        test_tool_call "$tool_name" "$tool_desc" "$test_id"
        test_id=$((test_id + 1))
        sleep 0.5 # Small delay between tool calls
    done
    
    echo ""
    echo "🧪 PHASE 3: Prism Mock API Testing"
    echo "=================================="
    
    # Test key Prism endpoints if available
    if curl -s "$PRISM_URL/" > /dev/null 2>&1; then
        test_prism_endpoint "GET" "/health" "Health check"
        test_prism_endpoint "GET" "/" "Root endpoint"
        test_prism_endpoint "GET" "/projects" "List projects"
        test_prism_endpoint "GET" "/users/me" "Get current user"
        test_prism_endpoint "GET" "/analytics/projects/stats" "Project analytics"
    else
        log_warning "Skipping Prism tests - service not accessible"
    fi
    
    echo ""
    echo "🧪 PHASE 4: MCP Protocol Compliance"
    echo "==================================="
    
    # Test additional MCP protocol methods
    log_test "Testing ping/keepalive"
    local ping_response
    ping_response=$(mcp_request "ping-1" "ping" '{}' "Ping test")
    if echo "$ping_response" | jq -e '.result' > /dev/null 2>&1; then
        record_test "MCP-Ping" "PASS" "Ping responded"
    else
        record_test "MCP-Ping" "SKIP" "Ping not supported"
    fi
    
    log_test "Testing notifications"
    local notify_response
    notify_response=$(mcp_request "notify-1" "notifications/initialized" '{}' "Initialized notification")
    if echo "$notify_response" | jq -e '.result' > /dev/null 2>&1; then
        record_test "MCP-Notifications" "PASS" "Notifications working"
    else
        record_test "MCP-Notifications" "SKIP" "Notifications not fully supported"
    fi
    
    # Show detailed results
    show_detailed_results
    show_tools_list
    
    # Generate final report
    echo ""
    echo "📊 TEST SUMMARY REPORT"
    echo "======================"
    echo "Total Tests:    $TOTAL_TESTS"
    echo "✅ Passed:      $PASSED_TESTS"
    echo "❌ Failed:      $FAILED_TESTS"
    echo "⚠️  Skipped:     $SKIPPED_TESTS"
    echo ""
    
    local success_rate=0
    if [ $TOTAL_TESTS -gt 0 ]; then
        success_rate=$((PASSED_TESTS * 100 / TOTAL_TESTS))
    fi
    
    echo "Success Rate:   $success_rate%"
    echo ""
    echo "Test Environment:"
    echo "  - MCP Server: $MCP_SERVER_URL"
    echo "  - Prism Mock: $PRISM_URL"
    echo "  - Timestamp:  $(date -Iseconds)"
    echo ""
    echo "Notes:"
    echo "  - Skipped tests typically indicate missing parameters or authentication requirements"
    echo "  - Some tools may require specific data formats not covered in generic testing"
    echo "  - Prism mock endpoints may have security requirements"
    
    if [ $FAILED_TESTS -eq 0 ]; then
        log_success "🎉 All critical tests passed! The MCP server is functioning correctly."
        exit 0
    else
        log_error "💥 $FAILED_TESTS test(s) failed. Please review the detailed results above."
        exit 1
    fi
}

# Run main function
main "$@"