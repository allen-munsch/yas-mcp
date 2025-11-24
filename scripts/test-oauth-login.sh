#!/bin/bash

# test-oauth-login.sh - Test the complete OAuth login flow

set -e

echo "🔐 Testing OAuth Login Flow"
echo "============================"

source .env

# Generate state and code verifier for PKCE
STATE=$(openssl rand -hex 16)
CODE_VERIFIER=$(openssl rand -hex 32)

echo "🔧 Generated OAuth parameters:"
echo "   State: $STATE"
echo "   Code Verifier: $CODE_VERIFIER"
echo ""

# Build authorization URL
AUTH_URL="${OAUTH_AUTH_URL}?response_type=code&client_id=${OAUTH_CLIENT_ID}&redirect_uri=http://localhost:8081/oauth/callback&scope=openid%20email%20profile&state=${STATE}"

echo "🌐 Authorization URL:"
echo "   $AUTH_URL"
echo ""
echo "📝 Manual testing required:"
echo "   1. Open the URL above in your browser"
echo "   2. Login with Keycloak credentials"
echo "   3. Copy the authorization code from the redirect URL"
echo ""
read -p "📋 Paste the authorization code here: " AUTH_CODE

if [ -z "$AUTH_CODE" ]; then
    echo "❌ No authorization code provided"
    exit 1
fi

echo ""
echo "🔄 Exchanging authorization code for tokens..."

# Exchange code for tokens
TOKEN_RESPONSE=$(curl -s -X POST \
  "$OAUTH_TOKEN_URL" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=authorization_code" \
  -d "client_id=$OAUTH_CLIENT_ID" \
  -d "client_secret=$OAUTH_CLIENT_SECRET" \
  -d "code=$AUTH_CODE" \
  -d "redirect_uri=http://localhost:8081/oauth/callback")

echo "$TOKEN_RESPONSE" | jq '.'

if echo "$TOKEN_RESPONSE" | jq -e '.access_token' > /dev/null; then
    ACCESS_TOKEN=$(echo "$TOKEN_RESPONSE" | jq -r '.access_token')
    echo ""
    echo "✅ Successfully obtained access token!"
    
    # Test user info endpoint
    echo ""
    echo "👤 Getting user information..."
    USER_INFO=$(curl -s -X GET \
      "$OAUTH_USER_INFO_URL" \
      -H "Authorization: Bearer $ACCESS_TOKEN")
    
    echo "$USER_INFO" | jq '.'
    
    if echo "$USER_INFO" | jq -e '.sub' > /dev/null || echo "$USER_INFO" | jq -e '.email' > /dev/null; then
        echo "✅ Successfully retrieved user info"
    else
        echo "⚠️  User info response unexpected"
    fi
    
    # Test API with access token
    echo ""
    echo "🔗 Testing API with access token..."
    API_RESPONSE=$(curl -s -X GET \
      "http://localhost:4010/users/me" \
      -H "Authorization: Bearer $ACCESS_TOKEN" \
      -H "Content-Type: application/json")
    
    echo "API Response: $API_RESPONSE"
    
else
    echo "❌ Failed to exchange authorization code"
    echo "Response: $TOKEN_RESPONSE"
    exit 1
fi

echo ""
echo "🎉 OAuth login flow test completed successfully!"
