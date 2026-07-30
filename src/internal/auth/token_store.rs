//! Token Store — automatic OAuth2/OIDC token lifecycle management.
//!
//! Provides in-memory token caching with automatic refresh, session binding,
//! and garbage collection of expired tokens.

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// An OAuth2/OIDC token set with lifecycle metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    /// Access token (the one used for API calls)
    pub access_token: String,

    /// Token type (usually "Bearer")
    pub token_type: String,

    /// Refresh token (may be None for client_credentials or implicit flows)
    pub refresh_token: Option<String>,

    /// ID token (OpenID Connect — contains user identity claims)
    pub id_token: Option<String>,

    /// When the access token was obtained
    pub obtained_at: DateTime<Utc>,

    /// When the access token expires
    pub expires_at: DateTime<Utc>,

    /// Scopes this token was granted
    pub scope: Option<String>,

    /// Provider name that issued this token
    pub provider: String,
}

impl TokenSet {
    /// Check if the token is expired or will expire within the buffer window
    pub fn is_expired(&self, refresh_buffer: Duration) -> bool {
        let now = Utc::now();
        let buffer_time =
            chrono::Duration::from_std(refresh_buffer).unwrap_or(chrono::Duration::minutes(5));
        now + buffer_time >= self.expires_at
    }

    /// Check if this token can be refreshed
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }

    /// Time until expiry
    pub fn time_to_expiry(&self) -> chrono::Duration {
        let now = Utc::now();
        if now >= self.expires_at {
            chrono::Duration::zero()
        } else {
            self.expires_at - now
        }
    }
}

/// A token entry stored in the cache
#[derive(Debug, Clone)]
struct TokenEntry {
    tokens: TokenSet,
    session_id: String,
    created_at: DateTime<Utc>,
}

/// Thread-safe token store with session management.
///
/// Uses `DashMap` for concurrent reads + `RwLock` for write-heavy operations.
/// Supports:
/// - Store tokens by session ID
/// - Retrieve tokens by session ID
/// - Automatic refresh detection (caller decides when to refresh)
/// - Session invalidation/revoke
/// - Garbage collection of expired entries
#[derive(Debug, Clone)]
pub struct TokenStore {
    /// session_id → TokenEntry
    sessions: Arc<DashMap<String, TokenEntry>>,
    /// access_token → session_id (reverse lookup)
    tokens_to_session: Arc<DashMap<String, String>>,
    /// Configuration
    config: Arc<RwLock<TokenStoreConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStoreConfig {
    /// Session TTL in seconds (default: 3600 = 1 hour)
    pub session_ttl: u64,

    /// Refresh buffer in seconds — refresh when token is within this many seconds of expiry
    pub refresh_buffer: u64,

    /// Max sessions per user/provider
    pub max_sessions_per_user: u64,
}

impl Default for TokenStoreConfig {
    fn default() -> Self {
        Self {
            session_ttl: 3600,
            refresh_buffer: 300, // 5 minutes
            max_sessions_per_user: 10,
        }
    }
}

impl TokenStore {
    /// Create a new token store
    pub fn new(config: TokenStoreConfig) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            tokens_to_session: Arc::new(DashMap::new()),
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Create a new session and store tokens
    ///
    /// Returns a session ID that can be used for subsequent calls.
    pub fn create_session(&self, tokens: TokenSet, user_id: &str) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        let entry = TokenEntry {
            tokens,
            session_id: session_id.clone(),
            created_at: Utc::now(),
        };

        let access_token = entry.tokens.access_token.clone();

        // Check max sessions
        if self.sessions.len() as u64
            >= self
                .config
                .try_read()
                .map(|c| c.max_sessions_per_user)
                .unwrap_or(10)
                * 10
        {
            warn!(user_id = %user_id, "High session count — consider GC");
        }

        self.sessions.insert(session_id.clone(), entry);
        self.tokens_to_session
            .insert(access_token, session_id.clone());

        info!(
            session_id = %session_id,
            user_id = %user_id,
            provider = %self.sessions.get(&session_id).map(|e| e.tokens.provider.clone()).unwrap_or_default(),
            "Session created"
        );

        Ok(session_id)
    }

    /// Get tokens for a session
    pub fn get(&self, session_id: &str) -> Option<TokenSet> {
        self.sessions.get(session_id).map(|e| {
            // Return tokens even if session is expired — caller should check
            // Expired sessions will be garbage collected periodically
            e.tokens.clone()
        })
    }

    /// Look up tokens by access token value
    pub fn get_by_access_token(&self, access_token: &str) -> Option<TokenSet> {
        self.tokens_to_session
            .get(access_token)
            .and_then(|session_id| self.get(&session_id))
    }

    /// Update tokens for a session (e.g., after a refresh)
    pub fn update(&self, session_id: &str, new_tokens: TokenSet) -> Result<()> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("Session {} not found", session_id))?;

        // Update reverse lookup
        self.tokens_to_session.remove(&entry.tokens.access_token);
        self.tokens_to_session
            .insert(new_tokens.access_token.clone(), session_id.to_string());

        entry.tokens = new_tokens;

        debug!(session_id = %session_id, "Token updated");
        Ok(())
    }

    /// Revoke a session (logout)
    pub fn revoke(&self, session_id: &str) -> Result<()> {
        if let Some((_, entry)) = self.sessions.remove(session_id) {
            self.tokens_to_session.remove(&entry.tokens.access_token);
            info!(session_id = %session_id, "Session revoked");
        }
        Ok(())
    }

    /// Get the number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Check if a token needs refreshing
    pub fn needs_refresh(&self, session_id: &str) -> Option<bool> {
        self.get(session_id).map(|tokens| {
            let refresh_buffer = Duration::from_secs(
                self.config
                    .try_read()
                    .map(|c| c.refresh_buffer)
                    .unwrap_or(300),
            );
            tokens.is_expired(refresh_buffer)
        })
    }

    /// Garbage collect expired sessions and tokens
    pub fn garbage_collect(&self) -> usize {
        let session_ttl = self
            .config
            .try_read()
            .map(|c| c.session_ttl)
            .unwrap_or(3600);
        let now = Utc::now();
        let ttl_duration = chrono::Duration::seconds(session_ttl as i64);

        let mut removed = 0;
        let to_remove: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| entry.created_at + ttl_duration < now)
            .map(|entry| entry.session_id.clone())
            .collect();

        for session_id in to_remove {
            if let Some((_, entry)) = self.sessions.remove(&session_id) {
                self.tokens_to_session.remove(&entry.tokens.access_token);
                removed += 1;
            }
        }

        if removed > 0 {
            info!(removed = removed, "Garbage collected expired sessions");
        }

        removed
    }
}

/// Refresh an access token using the refresh_token grant.
///
/// Calls the token endpoint and returns a new `TokenSet`.
pub async fn refresh_access_token(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
    client: &Client,
) -> Result<TokenSet> {
    info!(endpoint = %token_endpoint, "Refreshing access token");

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];

    let response = client
        .post(token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to refresh token: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Token refresh returned HTTP {}: {}",
            status,
            error_body
        ));
    }

    #[derive(Deserialize)]
    struct RefreshResponse {
        access_token: String,
        token_type: Option<String>,
        expires_in: Option<u64>,
        refresh_token: Option<String>,
        scope: Option<String>,
        id_token: Option<String>,
    }

    let refresh_resp: RefreshResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse refresh token response: {}", e))?;

    let expires_in = refresh_resp.expires_in.unwrap_or(3600);
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(expires_in as i64);

    Ok(TokenSet {
        access_token: refresh_resp.access_token,
        token_type: refresh_resp
            .token_type
            .unwrap_or_else(|| "Bearer".to_string()),
        refresh_token: refresh_resp
            .refresh_token
            .or(Some(refresh_token.to_string())),
        id_token: refresh_resp.id_token,
        obtained_at: now,
        expires_at,
        scope: refresh_resp.scope,
        provider: String::new(), // Caller should set this
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_expiry() {
        let tokens = TokenSet {
            access_token: "test".into(),
            token_type: "Bearer".into(),
            refresh_token: Some("refresh".into()),
            id_token: None,
            obtained_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            scope: None,
            provider: "test".into(),
        };

        // Not expired with default buffer
        assert!(!tokens.is_expired(Duration::from_secs(300)));

        // Expired with a very large buffer
        assert!(tokens.is_expired(Duration::from_secs(600)));
        assert!(tokens.can_refresh());
    }

    #[test]
    fn test_token_no_refresh() {
        let tokens = TokenSet {
            access_token: "test".into(),
            token_type: "Bearer".into(),
            refresh_token: None,
            id_token: None,
            obtained_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: None,
            provider: "test".into(),
        };

        assert!(!tokens.can_refresh());
    }

    #[test]
    fn test_token_store_create_and_get() {
        let store = TokenStore::new(TokenStoreConfig::default());
        let tokens = TokenSet {
            access_token: "acc_123".into(),
            token_type: "Bearer".into(),
            refresh_token: Some("ref_456".into()),
            id_token: None,
            obtained_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            scope: None,
            provider: "github".into(),
        };

        let session_id = store.create_session(tokens, "user-1").unwrap();
        assert!(store.get(&session_id).is_some());
        assert!(store.get_by_access_token("acc_123").is_some());
        assert_eq!(store.session_count(), 1);

        store.revoke(&session_id).unwrap();
        assert!(store.get(&session_id).is_none());
        assert_eq!(store.session_count(), 0);
    }
}

// ── Connector Token Cache ──────────────────────────────────────────

/// A lightweight cache mapping connector names to OAuth2 access tokens.
/// Used by the WIMSE token exchange endpoint to serve tokens to sandboxes.
///
/// Weft pushes tokens via `POST /api/auth/tokens`.
/// Sandboxes retrieve tokens via `POST /api/auth/exchange` (with platform JWT).
#[derive(Debug, Clone, Default)]
pub struct ConnectorTokenCache {
    /// connector_name → (access_token, expires_at)
    tokens: Arc<DashMap<String, (String, DateTime<Utc>)>>,
}

impl ConnectorTokenCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
        }
    }

    /// Store a token for a connector.
    pub fn store(&self, connector: &str, access_token: &str, expires_in_secs: i64) {
        let expires_at = Utc::now() + chrono::Duration::seconds(expires_in_secs);
        self.tokens.insert(
            connector.to_string(),
            (access_token.to_string(), expires_at),
        );
        info!(
            connector,
            expires_in = expires_in_secs,
            "connector token stored"
        );
    }

    /// Get a valid token for a connector, if available and not expired.
    pub fn get(&self, connector: &str) -> Option<String> {
        self.tokens.get(connector).and_then(|entry| {
            let (token, expires_at) = entry.value();
            if Utc::now() < *expires_at {
                Some(token.clone())
            } else {
                debug!(connector, "connector token expired, removing");
                drop(entry);
                self.tokens.remove(connector);
                None
            }
        })
    }

    /// Remove a token for a connector.
    pub fn remove(&self, connector: &str) {
        self.tokens.remove(connector);
    }

    /// Number of cached connectors.
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

#[cfg(test)]
mod connector_cache_tests {
    use super::*;

    #[test]
    fn test_store_and_get() {
        let cache = ConnectorTokenCache::new();
        cache.store("gmail", "gm_token_123", 3600);
        assert_eq!(cache.get("gmail"), Some("gm_token_123".into()));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_expired_token_returns_none() {
        let cache = ConnectorTokenCache::new();
        cache.store("slack", "sl_token_456", -1); // already expired
        assert_eq!(cache.get("slack"), None);
        assert!(cache.is_empty()); // auto-removed
    }

    #[test]
    fn test_remove() {
        let cache = ConnectorTokenCache::new();
        cache.store("github", "gh_token_789", 3600);
        assert_eq!(cache.len(), 1);
        cache.remove("github");
        assert!(cache.is_empty());
    }

    #[test]
    fn test_missing_connector() {
        let cache = ConnectorTokenCache::new();
        assert_eq!(cache.get("nonexistent"), None);
    }
}
