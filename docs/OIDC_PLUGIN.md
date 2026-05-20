# OIDC Plug-and-Play Design

> **Goal**: Make OIDC onboarding trivial — paste a provider URL, get full OAuth2/OIDC protection. No code changes needed.

## Current State (v0.1.0)

Yas-mcp supports OAuth2 via explicit provider config in `config.yaml` or `mcp-oauth-config.yaml`:

```yaml
oauth:
  enabled: true
  provider: github              # one of: github, google, microsoft, generic
  client_id: your_client_id
  client_secret: your_client_secret
  scopes:
    - read:user
    - user:email
  redirect_uri: http://localhost:3000/oauth/callback
```

For `generic`, you must manually specify auth/token/userinfo URLs:

```yaml
oauth:
  enabled: true
  provider: generic
  auth_url: https://my-idp.example.com/oauth/authorize
  token_url: https://my-idp.example.com/oauth/token
  user_info_url: https://my-idp.example.com/userinfo
  client_id: ...
  client_secret: ...
  scopes: [openid, profile, email]
```

## Target State: OIDC Discovery

### Configuration

```yaml
oidc:
  providers:
    - name: corporate-sso
      issuer_url: https://auth.corp.example.com    # .well-known auto-discovered
      client_id: ${CORP_CLIENT_ID}
      client_secret: ${CORP_CLIENT_SECRET}
      scopes: [openid, profile, email]
      route_filter: /api/corp/*                      # only protect corporate routes

    - name: partner-api
      issuer_url: https://partner.auth.example.com
      client_id: ${PARTNER_CLIENT_ID}
      client_secret: ${PARTNER_CLIENT_SECRET}
      route_filter: /api/partner/*

    - name: public
      auth: none                                      # unauthenticated routes
      route_filter: /api/public/*
```

### Discovery Process

```
1. User provides: issuer_url
       ↓
2. yas-mcp fetches: GET {issuer_url}/.well-known/openid-configuration
       ↓
3. Parse response:
   {
     "issuer": "https://auth.corp.example.com",
     "authorization_endpoint": "https://auth.corp.example.com/authorize",
     "token_endpoint": "https://auth.corp.example.com/token",
     "userinfo_endpoint": "https://auth.corp.example.com/userinfo",
     "jwks_uri": "https://auth.corp.example.com/jwks",
     "scopes_supported": ["openid", "profile", "email"],
     "response_types_supported": ["code", "token", "id_token"],
     "grant_types_supported": ["authorization_code", "refresh_token"],
     ...
   }
       ↓
4. Auto-configure: provider created with all endpoints resolved
       ↓
5. JWKS cached: keys fetched, parsed, cached with TTL from HTTP headers
       ↓
6. Ready: provider available for MCP tool auth gating
```

### Multi-Tenant Routing

```yaml
oidc:
  default_provider: corporate-sso    # fallback for unmatched routes

  providers:
    - name: corporate-sso
      issuer_url: https://auth.corp.example.com
      route_filter: /api/corp/**
      # Tools matching /api/corp/* use this provider

    - name: partner-api
      issuer_url: https://partner.auth.example.com
      route_filter: /api/partner/**
      # Tools matching /api/partner/* use this provider
```

### Token Lifecycle

```
┌──────────┐     ┌──────────┐     ┌──────────┐
│  Client  │     │ yas-mcp  │     │   OIDC   │
│  (MCP)   │     │  Proxy   │     │ Provider │
└────┬─────┘     └────┬─────┘     └────┬─────┘
     │                │                │
     │ 1. tools/call  │                │
     │  (no token)    │                │
     ├───────────────►│                │
     │                │ 2. 401 +       │
     │                │    auth_url    │
     │◄───────────────│                │
     │                │                │
     │ 3. Browser     │                │
     │    OIDC flow   │                │
     ├─────────────────────────────────►
     │                │                │
     │ 4. Auth code   │                │
     │◄────────────────────────────────┤
     │                │                │
     │ 5. tools/call  │                │
     │  (auth_code)   │                │
     ├───────────────►│                │
     │                │ 6. Exchange    │
     │                │    code→tokens │
     │                ├───────────────►│
     │                │◄───────────────┤
     │                │                │
     │                │ 7. Cache       │
     │                │    tokens      │
     │                │                │
     │ 8. Result      │                │
     │◄───────────────│                │
     │                │                │
     │ 9. tools/call  │                │
     │  (session_id)  │                │
     ├───────────────►│                │
     │                │ 10. Use cached │
     │                │     token      │
     │                │                │
     │ 11. Result     │                │
     │◄───────────────│                │
```

### Implementation Plan

#### Step 1: OIDC Discovery Module
```rust
// src/internal/auth/oidc_discovery.rs

pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
    pub scopes_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
}

impl OidcDiscovery {
    /// Fetch and parse .well-known/openid-configuration
    pub async fn discover(issuer_url: &str) -> Result<Self> { ... }
}
```

#### Step 2: JWKS Cache
```rust
// src/internal/auth/jwks.rs

pub struct JwksCache {
    keys: Vec<Jwk>,
    expires_at: Instant,
}

impl JwksCache {
    /// Fetch JWKS, cache with TTL
    pub async fn fetch(jwks_uri: &str) -> Result<Self> { ... }
    /// Verify JWT signature using cached keys
    pub fn verify(&self, token: &str) -> Result<Claims> { ... }
}
```

#### Step 3: Token Store
```rust
// src/internal/auth/token_store.rs

pub struct TokenStore {
    // access_token → (refresh_token, expires_at, provider)
    tokens: DashMap<String, TokenEntry>,
    // session_id → access_token
    sessions: DashMap<String, String>,
}

impl TokenStore {
    pub fn store(&self, session_id: &str, tokens: TokenSet) { ... }
    pub fn get(&self, session_id: &str) -> Option<AccessToken> { ... }
    pub fn refresh(&self, session_id: &str) -> Result<AccessToken> { ... }
    pub fn revoke(&self, session_id: &str) { ... }
}
```

#### Step 4: Multi-Provider Router
```rust
// src/internal/auth/provider_router.rs

pub struct ProviderRouter {
    providers: Vec<RouteProvider>,
    default: String,
}

struct RouteProvider {
    name: String,
    provider: Arc<OidcProvider>,
    route_pattern: GlobPattern,
}

impl ProviderRouter {
    /// Match tool path → provider
    pub fn resolve(&self, tool_path: &str) -> &OidcProvider { ... }
}
```

### Config Schema (Target)

```yaml
oidc:
  # Session management
  session:
    ttl: 3600                    # seconds
    refresh_buffer: 300          # refresh when <5min remaining
    max_sessions_per_user: 10

  # Token validation
  validation:
    verify_iss: true
    verify_aud: true
    verify_exp: true
    leeway: 60                   # clock skew tolerance

  # Provider definitions
  providers:
    - name: default
      issuer_url: https://auth.example.com
      client_id: ${OIDC_CLIENT_ID}
      client_secret: ${OIDC_CLIENT_SECRET}
      scopes: [openid, profile, email]
      # Optional overrides (if not using .well-known)
      # authorization_endpoint: ...
      # token_endpoint: ...
      # jwks_uri: ...
      route_filter: "/**"        # default for all routes
```

### Provider Registry (No-Code Provider Addition)

```yaml
# providers.yaml — drop this file to add new providers
# yas-mcp auto-loads on startup

providers:
  okta:
    issuer_url_template: "https://{domain}/oauth2/default"
    doc_url: "https://developer.okta.com/docs/reference/api/oidc/"
    
  auth0:
    issuer_url_template: "https://{domain}"
    doc_url: "https://auth0.com/docs/authenticate/protocols/openid-connect-protocol"
    
  azure_ad:
    issuer_url_template: "https://login.microsoftonline.com/{tenant_id}/v2.0"
    doc_url: "https://learn.microsoft.com/en-us/entra/identity-platform/v2-protocols-oidc"
    
  keycloak:
    issuer_url_template: "https://{host}:{port}/realms/{realm}"
    doc_url: "https://www.keycloak.org/docs/latest/server_admin/"
```

This registry means adding a new provider is purely a config change — no Rust code needed.
