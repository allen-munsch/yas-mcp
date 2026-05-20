#!/bin/bash

# test-oauth-flow.sh - Comprehensive OAuth2 integration test

set -e

echo "🧪 OAuth2 Integration Test Suite"
echo "================================"

# Source environment
if [ -f .env ]; then
    set -a
    source .env
    set +a
else
    echo "❌ .env file not found"
    exit 1
fi

# Check if services are running
echo ""
echo "🔍 Checking services..."
if ! curl -s http://localhost:8081/realms/master > /dev/null; then
    echo "❌ Keycloak is not running on localhost:8081"
    echo "   Start with: docker-compose --profile auth up -d keycloak"
    exit 1
fi

if ! curl -s http://localhost:4010/health > /dev/null; then
    echo "❌ Prism mock server is not running on localhost:4010"
    echo "   Start with: docker-compose up -d prism-mock"
    exit 1
fi

echo "✅ All services are running"

# Test 1: Keycloak Admin API Access
echo ""
echo "1. 🔑 Testing Keycloak Admin API..."
ADMIN_TOKEN=$(curl -s -X POST \
  "http://localhost:8081/realms/master/protocol/openid-connect/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "username=${KEYCLOAK_ADMIN}" \
  -d "password=${KEYCLOAK_ADMIN_PASSWORD}" \
  -d "grant_type=password" \
  -d "client_id=admin-cli" | jq -r '.access_token')

if [ "$ADMIN_TOKEN" = "null" ] || [ -z "$ADMIN_TOKEN" ]; then
    echo "❌ Failed to get admin token"
    exit 1
fi
echo "✅ Admin token obtained successfully"

# Test 2: Check OAuth Client Exists
echo ""
echo "2. 🔍 Testing OAuth Client Configuration..."
CLIENT_CHECK=$(curl -s -X GET \
  "http://localhost:8081/admin/realms/master/clients" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json")

if echo "$CLIENT_CHECK" | jq -e ".[] | select(.clientId == \"$OAUTH_CLIENT_ID\")" > /dev/null; then
    echo "✅ OAuth client '$OAUTH_CLIENT_ID' exists"
else
    echo "❌ OAuth client '$OAUTH_CLIENT_ID' not found"
    echo "   Run: ./setup-oauth-client.sh"
    exit 1
fi

# Test 3: Test OAuth Endpoints
echo ""
echo "3. 🌐 Testing OAuth Endpoints..."

# Test Authorization endpoint
AUTH_RESPONSE=$(curl -s -I "$OAUTH_AUTH_URL?response_type=code&client_id=$OAUTH_CLIENT_ID&redirect_uri=http://localhost:8081/oauth/callback&scope=openid" | head -1)
if echo "$AUTH_RESPONSE" | grep -q "200"; then
    echo "✅ Authorization endpoint is accessible"
else
    echo "❌ Authorization endpoint failed: $AUTH_RESPONSE"
fi

# Test Token endpoint (with invalid code to check if endpoint works)
TOKEN_RESPONSE=$(curl -s -X POST \
  "$OAUTH_TOKEN_URL" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code" \
  -d "client_id=$OAUTH_CLIENT_ID" \
  -d "client_secret=$OAUTH_CLIENT_SECRET" \
  -d "code=invalid_code" \
  -d "redirect_uri=http://localhost:8081/oauth/callback")

if echo "$TOKEN_RESPONSE" | jq -e '.error' > /dev/null; then
    echo "✅ Token endpoint is accessible (correctly rejected invalid code)"
else
    echo "⚠️  Token endpoint response unexpected: $TOKEN_RESPONSE"
fi

# Test 4: Test MCP Server with OAuth Config
echo ""
echo "4. 🚀 Testing MCP Server with OAuth Configuration..."

# Check if MCP server is running
if curl -s http://localhost:3000/health > /dev/null; then
    echo "✅ MCP server is running on localhost:3000"
    
    # Test MCP initialization
    MCP_INIT=$(curl -s -X POST \
      "http://localhost:3000/mcp" \
      -H "Content-Type: application/json" \
      -d '{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
          "protocolVersion": "2024-11-05",
          "capabilities": {},
          "clientInfo": {
            "name": "oauth-test",
            "version": "1.0.0"
          }
        }
      }')
    
    if echo "$MCP_INIT" | jq -e '.result' > /dev/null; then
        echo "✅ MCP server initialized successfully"
    else
        echo "❌ MCP server initialization failed"
        echo "Response: $MCP_INIT"
    fi
else
    echo "⚠️  MCP server not running on localhost:3000"
    echo "   Start with: cargo run --bin yas-mcp -- --config mcp-oauth-config.yaml --swagger-file examples/todo-app/openapi.yaml --mode http"
fi

# Test 5: Test API endpoints with authentication
echo ""
echo "5. 🔐 Testing API Endpoints with Authentication..."

# Test unauthenticated request (should work for some endpoints)
UNAUTH_RESPONSE=$(curl -s -X GET \
  "http://localhost:4010/health" \
  -H "Content-Type: application/json")

if echo "$UNAUTH_RESPONSE" | jq -e '.status' > /dev/null; then
    echo "✅ Unauthenticated health check works"
else
    echo "⚠️  Health check failed: $UNAUTH_RESPONSE"
fi

# Test authenticated endpoint (should fail without token)
AUTH_REQUIRED_RESPONSE=$(curl -s -X GET \
  "http://localhost:4010/users/me" \
  -H "Content-Type: application/json")

if echo "$AUTH_REQUIRED_RESPONSE" | jq -e '.error' > /dev/null || [ "$AUTH_REQUIRED_RESPONSE" = "Unauthorized" ]; then
    echo "✅ Authentication required for protected endpoints"
else
    echo "⚠️  Unexpected response from protected endpoint: $AUTH_REQUIRED_RESPONSE"
fi

echo ""
echo "🎉 OAuth2 Integration Test Complete!"
echo "===================================="
echo ""
echo "📋 Next steps:"
echo "   1. Test the full OAuth flow with: ./test-oauth-login.sh"
echo "   2. Verify MCP tools work: ./test-mcp-tools.sh"
echo "   3. Test with actual OAuth login: ./test-oauth-login-flow.sh"
