#!/bin/bash

# setup-oauth-client.sh - Create OAuth client in Keycloak

set -e

echo "🔐 Setting up OAuth Client in Keycloak..."
echo "=========================================="

# Source environment variables
if [ -f .env ]; then
    set -a
    source .env
    set +a
else
    echo "❌ .env file not found. Please run setup-keycloak.sh first."
    exit 1
fi

# Default values
KEYCLOAK_URL=${KEYCLOAK_URL:-"http://localhost:8081"}
REALM=${REALM:-"master"}
CLIENT_NAME=${CLIENT_NAME:-"mcp-client"}
CLIENT_ID=${CLIENT_ID:-"mcp-client"}
REDIRECT_URIS=${REDIRECT_URIS:-"http://localhost:8081/oauth/callback,http://127.0.0.1:8081/oauth/callback"}

# Generate client secret
CLIENT_SECRET=$(openssl rand -base64 32 | tr -d '/+=' | cut -c1-32)

echo "🔧 Configuration:"
echo "   Keycloak URL: $KEYCLOAK_URL"
echo "   Realm: $REALM"
echo "   Client Name: $CLIENT_NAME"
echo "   Client ID: $CLIENT_ID"
echo "   Redirect URIs: $REDIRECT_URIS"
echo ""

# Get admin token
echo "🔑 Getting admin access token..."
ADMIN_TOKEN=$(curl -s -X POST \
  "${KEYCLOAK_URL}/realms/master/protocol/openid-connect/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "username=${KEYCLOAK_ADMIN}" \
  -d "password=${KEYCLOAK_ADMIN_PASSWORD}" \
  -d "grant_type=password" \
  -d "client_id=admin-cli" | jq -r '.access_token')

if [ "$ADMIN_TOKEN" = "null" ] || [ -z "$ADMIN_TOKEN" ]; then
    echo "❌ Failed to get admin token. Please check:"
    echo "   - Keycloak is running at $KEYCLOAK_URL"
    echo "   - Admin credentials are correct in .env"
    exit 1
fi

echo "✅ Admin token obtained successfully"
echo ""

# Check if client already exists
echo "🔍 Checking if client '$CLIENT_ID' already exists..."
EXISTING_CLIENT=$(curl -s -X GET \
  "${KEYCLOAK_URL}/admin/realms/${REALM}/clients" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" | jq -r ".[] | select(.clientId == \"$CLIENT_ID\") | .id")

if [ -n "$EXISTING_CLIENT" ]; then
    echo "⚠️  Client '$CLIENT_ID' already exists. Updating..."
    
    # Update existing client
    curl -s -X PUT \
      "${KEYCLOAK_URL}/admin/realms/${REALM}/clients/$EXISTING_CLIENT" \
      -H "Authorization: Bearer $ADMIN_TOKEN" \
      -H "Content-Type: application/json" \
      -d "{
        \"clientId\": \"$CLIENT_ID\",
        \"name\": \"$CLIENT_NAME\",
        \"secret\": \"$CLIENT_SECRET\",
        \"enabled\": true,
        \"protocol\": \"openid-connect\",
        \"publicClient\": false,
        \"redirectUris\": [\"$(echo $REDIRECT_URIS | sed 's/,/\",\"/g')\"],
        \"webOrigins\": [\"+\"],
        \"standardFlowEnabled\": true,
        \"implicitFlowEnabled\": false,
        \"directAccessGrantsEnabled\": true,
        \"serviceAccountsEnabled\": true
      }" > /dev/null
else
    echo "📝 Creating new client '$CLIENT_ID'..."
    
    # Create new client
    curl -s -X POST \
      "${KEYCLOAK_URL}/admin/realms/${REALM}/clients" \
      -H "Authorization: Bearer $ADMIN_TOKEN" \
      -H "Content-Type: application/json" \
      -d "{
        \"clientId\": \"$CLIENT_ID\",
        \"name\": \"$CLIENT_NAME\",
        \"secret\": \"$CLIENT_SECRET\",
        \"enabled\": true,
        \"protocol\": \"openid-connect\",
        \"publicClient\": false,
        \"redirectUris\": [\"$(echo $REDIRECT_URIS | sed 's/,/\",\"/g')\"],
        \"webOrigins\": [\"+\"],
        \"standardFlowEnabled\": true,
        \"implicitFlowEnabled\": false,
        \"directAccessGrantsEnabled\": true,
        \"serviceAccountsEnabled\": true
      }" > /dev/null
fi

echo "✅ OAuth client configured successfully!"
echo ""

# Get the client ID for the secret
CLIENT_UUID=$(curl -s -X GET \
  "${KEYCLOAK_URL}/admin/realms/${REALM}/clients" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" | jq -r ".[] | select(.clientId == \"$CLIENT_ID\") | .id")

echo "📋 OAuth Client Configuration:"
echo "=============================="
echo "Client Name: $CLIENT_NAME"
echo "Client ID: $CLIENT_ID"
echo "Client Secret: $CLIENT_SECRET"
echo "Realm: $REALM"
echo "Authorization URL: ${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/auth"
echo "Token URL: ${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/token"
echo "User Info URL: ${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/userinfo"
echo "Redirect URIs: $REDIRECT_URIS"
echo ""

# Append to .env file
cat >> .env << EOF

# OAuth Client Configuration
# Generated on $(date)
OAUTH_CLIENT_ID=$CLIENT_ID
OAUTH_CLIENT_SECRET=$CLIENT_SECRET
OAUTH_REALM=$REALM
OAUTH_AUTH_URL=${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/auth
OAUTH_TOKEN_URL=${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/token
OAUTH_USER_INFO_URL=${KEYCLOAK_URL}/realms/${REALM}/protocol/openid-connect/userinfo
EOF

echo "📁 Client configuration saved to .env file"
echo ""
echo "🚀 You can now use these OAuth credentials in your MCP server!"
