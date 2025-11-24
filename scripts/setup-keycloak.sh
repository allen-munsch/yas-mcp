#!/bin/bash

# setup-keycloak.sh - Setup Keycloak with random credentials

set -e  # Exit on any error

echo "🔐 Setting up Keycloak with credentials..."
echo "================================================"

# Generate credentials
KEYCLOAK_ADMIN="admin_$(openssl rand -hex 12)"
KEYCLOAK_ADMIN_PASSWORD=$(openssl rand -base64 128 | tr -d '\n' | tr -d '/+=' | cut -c1-128)

# Append to .env file (create if doesn't exist)
cat >> .env << EOF

# Keycloak Admin Credentials
# Generated on $(date)
KEYCLOAK_ADMIN=$KEYCLOAK_ADMIN
KEYCLOAK_ADMIN_PASSWORD=$KEYCLOAK_ADMIN_PASSWORD
EOF

echo "✅ Generated credentials:"
echo "   Admin: $KEYCLOAK_ADMIN"
echo "   Password: $KEYCLOAK_ADMIN_PASSWORD"
echo ""
echo "📁 Credentials appended to .env file"
echo "🔒 Make sure to keep the .env file and never commit it to version control!"
echo ""

# Make .env file read-only for security
chmod 600 .env

echo "🚀 To start Keycloak, run:"
echo "   docker-compose --profile auth up -d"
echo ""
echo "🌐 Keycloak will be available at: http://localhost:8081"