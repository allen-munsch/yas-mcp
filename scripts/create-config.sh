#!/bin/bash

# generate-mcp-config.sh - Generate MCP server configuration from OAuth client

set -e

echo "🎯 Generating MCP Server Configuration"
echo "======================================"

source .env

# Check if OAuth client is configured
if [ -z "$OAUTH_CLIENT_ID" ] || [ -z "$OAUTH_CLIENT_SECRET" ]; then
    echo "❌ OAuth client not configured. Please run setup-oauth-client.sh first."
    exit 1
fi

# Generate MCP config
cat > mcp-oauth-config.yaml << EOF
# MCP Server OAuth Configuration
# Generated on $(date)

endpoint:
  base_url: "http://127.0.0.1:4010"
  auth_type: "none"

oauth:
  enabled: true
  provider: "generic"
  client_id: "$OAUTH_CLIENT_ID"
  client_secret: "$OAUTH_CLIENT_SECRET"
  auth_url: "$OAUTH_AUTH_URL"
  token_url: "$OAUTH_TOKEN_URL"
  user_info_url: "$OAUTH_USER_INFO_URL"
  scopes: ["openid", "email", "profile"]
  redirect_uri: "http://localhost:8081/oauth/callback"
  allow_origins:
    - "http://127.0.0.1:6274"
    - "http://localhost:3000"

# Required fields
swagger_file: "/app/config/swagger.json"
adjustments_file: "/app/config/adjustments.yaml"
EOF

echo "✅ MCP OAuth configuration generated: mcp-oauth-config.yaml"
echo ""
echo "📋 To use this configuration:"
echo "   cp mcp-oauth-config.yaml config.yaml"
echo "   docker-compose up -d"
echo ""
echo "🔐 Your OAuth endpoints:"
echo "   Auth: $OAUTH_AUTH_URL"
echo "   Token: $OAUTH_TOKEN_URL"
echo "   User Info: $OAUTH_USER_INFO_URL"
