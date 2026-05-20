//! API Belt — MCP tools for API lifecycle management.
//!
//! Phase 3: Agent Skills — The API Conveyor Belt.
//! These are meta-tools that any agent can call to surface, list, update,
//! or offboard APIs through yas-mcp.
//!
//! ## Tools
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `api_onboard` | Surface a new API from OpenAPI spec URL |
//! | `api_list` | List all onboarded APIs with status |
//! | `api_status` | Get detailed status of one API |
//! | `api_update` | Re-parse spec and hot-reload tools |
//! | `api_offboard` | Remove an API and its tools |
//! | `api_catalog` | Show catalog entries for onboarded APIs |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// ──────────────────────────────────────────────
//  Onboarded API record
// ──────────────────────────────────────────────

/// A record of an onboarded API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardedApi {
    /// Human-readable name (derived from spec or user-provided)
    pub name: String,

    /// Source of the OpenAPI spec (URL or file path)
    pub spec_url: String,

    /// When the API was onboarded
    pub onboarded_at: String,

    /// Number of MCP tools generated
    pub tool_count: usize,

    /// Tool names (for quick listing)
    pub tools: Vec<String>,

    /// Health status
    #[serde(default = "default_health")]
    pub health: ApiHealth,

    /// Number of tool calls served
    #[serde(default)]
    pub call_count: u64,

    /// Number of errors
    #[serde(default)]
    pub error_count: u64,

    /// Auth type
    #[serde(default)]
    pub auth_type: String,

    /// Base URL of the upstream API
    #[serde(default)]
    pub base_url: String,

    /// Arbitrary metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_health() -> ApiHealth {
    ApiHealth::Healthy
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApiHealth {
    Healthy,
    Degraded,
    Down,
    Unknown,
}

impl std::fmt::Display for ApiHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiHealth::Healthy => write!(f, "healthy"),
            ApiHealth::Degraded => write!(f, "degraded"),
            ApiHealth::Down => write!(f, "down"),
            ApiHealth::Unknown => write!(f, "unknown"),
        }
    }
}

// ──────────────────────────────────────────────
//  API Belt Registry
// ──────────────────────────────────────────────

/// Thread-safe registry of all onboarded APIs.
///
/// This is the central record of what APIs yas-mcp is currently proxying.
/// The ToolRegistry holds the actual tool executors; this holds the metadata.
#[derive(Debug, Clone)]
pub struct ApiBelt {
    apis: Arc<RwLock<HashMap<String, OnboardedApi>>>,
}

impl ApiBelt {
    /// Create a new empty API belt
    pub fn new() -> Self {
        Self {
            apis: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── api_onboard ──────────────────────────

    /// Onboard a new API.
    ///
    /// Returns the API name and tool count.
    pub async fn onboard(
        &self,
        spec_url: &str,
        name: Option<&str>,
        auth_type: Option<&str>,
        base_url: Option<&str>,
    ) -> Result<OnboardResult, ApiBeltError> {
        let api_name = name
            .map(|n| n.to_string())
            .unwrap_or_else(|| derive_name_from_url(spec_url));

        let mut apis = self.apis.write().await;

        // Check for duplicates
        if apis.contains_key(&api_name) {
            return Err(ApiBeltError::AlreadyExists(api_name));
        }

        let api = OnboardedApi {
            name: api_name.clone(),
            spec_url: spec_url.to_string(),
            onboarded_at: chrono::Utc::now().to_rfc3339(),
            tool_count: 0, // Will be updated after parsing
            tools: Vec::new(),
            health: ApiHealth::Unknown,
            call_count: 0,
            error_count: 0,
            auth_type: auth_type.unwrap_or("none").to_string(),
            base_url: base_url.unwrap_or("").to_string(),
            metadata: HashMap::new(),
        };

        apis.insert(api_name.clone(), api);

        info!(
            api = %api_name,
            spec = %spec_url,
            "API onboarded"
        );

        Ok(OnboardResult {
            name: api_name,
            spec_url: spec_url.to_string(),
            status: "onboarded".to_string(),
        })
    }

    // ── api_list ─────────────────────────────

    /// List all onboarded APIs with summary status.
    pub async fn list(&self) -> Vec<OnboardedApi> {
        self.apis.read().await.values().cloned().collect()
    }

    /// List as a compact summary
    pub async fn list_summary(&self) -> Vec<ApiSummary> {
        self.apis
            .read()
            .await
            .values()
            .map(|api| ApiSummary {
                name: api.name.clone(),
                tool_count: api.tool_count,
                health: api.health.clone(),
                auth_type: api.auth_type.clone(),
            })
            .collect()
    }

    // ── api_status ───────────────────────────

    /// Get detailed status of a specific API.
    pub async fn status(&self, name: &str) -> Result<OnboardedApi, ApiBeltError> {
        self.apis
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| ApiBeltError::NotFound(name.to_string()))
    }

    // ── api_update ───────────────────────────

    /// Mark an API as updated (e.g., after re-parsing its spec).
    ///
    /// Updates the tool count and tool names.
    pub async fn update_tools(
        &self,
        name: &str,
        tool_count: usize,
        tools: Vec<String>,
    ) -> Result<(), ApiBeltError> {
        let mut apis = self.apis.write().await;
        let api = apis
            .get_mut(name)
            .ok_or_else(|| ApiBeltError::NotFound(name.to_string()))?;

        let old_count = api.tool_count;
        api.tool_count = tool_count;
        api.tools = tools;

        info!(
            api = %name,
            old_tools = old_count,
            new_tools = tool_count,
            "API tools updated"
        );

        Ok(())
    }

    /// Update health status
    pub async fn update_health(
        &self,
        name: &str,
        health: ApiHealth,
    ) -> Result<(), ApiBeltError> {
        let mut apis = self.apis.write().await;
        let api = apis
            .get_mut(name)
            .ok_or_else(|| ApiBeltError::NotFound(name.to_string()))?;

        api.health = health;
        Ok(())
    }

    /// Increment call count
    pub async fn increment_calls(&self, name: &str) {
        if let Ok(mut apis) = self.apis.try_write() {
            if let Some(api) = apis.get_mut(name) {
                api.call_count += 1;
            }
        }
    }

    /// Increment error count
    pub async fn increment_errors(&self, name: &str) {
        if let Ok(mut apis) = self.apis.try_write() {
            if let Some(api) = apis.get_mut(name) {
                api.error_count += 1;
            }
        }
    }

    // ── api_offboard ─────────────────────────

    /// Offboard an API.
    pub async fn offboard(&self, name: &str) -> Result<OffboardResult, ApiBeltError> {
        let mut apis = self.apis.write().await;
        let api = apis
            .remove(name)
            .ok_or_else(|| ApiBeltError::NotFound(name.to_string()))?;

        info!(
            api = %name,
            tools = api.tool_count,
            "API offboarded"
        );

        Ok(OffboardResult {
            name: api.name,
            tool_count: api.tool_count,
            status: "offboarded".to_string(),
        })
    }

    // ── api_catalog ──────────────────────────

    /// Generate catalog-style summary of all APIs
    pub async fn catalog(&self, name: Option<&str>) -> Result<Vec<OnboardedApi>, ApiBeltError> {
        if let Some(name) = name {
            let api = self.status(name).await?;
            Ok(vec![api])
        } else {
            Ok(self.list().await)
        }
    }
}

impl Default for ApiBelt {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────────────────────────
//  Result types
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OnboardResult {
    pub name: String,
    pub spec_url: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OffboardResult {
    pub name: String,
    pub tool_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiSummary {
    pub name: String,
    pub tool_count: usize,
    pub health: ApiHealth,
    pub auth_type: String,
}

// ──────────────────────────────────────────────
//  Errors
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ApiBeltError {
    NotFound(String),
    AlreadyExists(String),
    InvalidSpec(String),
    DeployFailed(String),
}

impl std::fmt::Display for ApiBeltError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiBeltError::NotFound(name) => write!(f, "API not found: {}", name),
            ApiBeltError::AlreadyExists(name) => write!(f, "API already onboarded: {}", name),
            ApiBeltError::InvalidSpec(msg) => write!(f, "Invalid OpenAPI spec: {}", msg),
            ApiBeltError::DeployFailed(msg) => write!(f, "Deployment failed: {}", msg),
        }
    }
}

impl std::error::Error for ApiBeltError {}

// ──────────────────────────────────────────────
//  Helpers
// ──────────────────────────────────────────────

/// Derive an API name from its spec URL
fn derive_name_from_url(url: &str) -> String {
    // Try to extract a reasonable name from the URL
    let clean = url
        .trim_end_matches('/')
        .trim_end_matches("/openapi.json")
        .trim_end_matches("/openapi.yaml")
        .trim_end_matches("/swagger.json")
        .trim_end_matches("/swagger.yaml")
        .trim_end_matches("/api-docs")
        .trim_end_matches("/v1")
        .trim_end_matches("/v2")
        .trim_end_matches("/v3");

    // Take the last path segment
    clean
        .rsplit('/')
        .next()
        .unwrap_or("unknown-api")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_onboard_and_list() {
        let belt = ApiBelt::new();

        let result = belt
            .onboard("https://api.example.com/openapi.json", Some("Test API"), None, None)
            .await
            .unwrap();
        assert_eq!(result.name, "Test API");
        assert_eq!(result.status, "onboarded");

        let apis = belt.list().await;
        assert_eq!(apis.len(), 1);
        assert_eq!(apis[0].name, "Test API");
    }

    #[tokio::test]
    async fn test_onboard_duplicate_fails() {
        let belt = ApiBelt::new();
        belt.onboard("https://api.example.com/spec", Some("Dup"), None, None)
            .await
            .unwrap();

        let result = belt
            .onboard("https://api.example.com/spec2", Some("Dup"), None, None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_tools() {
        let belt = ApiBelt::new();
        belt.onboard("https://api.example.com/spec", Some("API"), None, None)
            .await
            .unwrap();

        belt.update_tools("API", 5, vec!["toolA".into(), "toolB".into()])
            .await
            .unwrap();

        let api = belt.status("API").await.unwrap();
        assert_eq!(api.tool_count, 5);
        assert_eq!(api.tools.len(), 2);
    }

    #[tokio::test]
    async fn test_offboard() {
        let belt = ApiBelt::new();
        belt.onboard("https://api.example.com/spec", Some("Temp"), None, None)
            .await
            .unwrap();

        let result = belt.offboard("Temp").await.unwrap();
        assert_eq!(result.status, "offboarded");

        let apis = belt.list().await;
        assert_eq!(apis.len(), 0);
    }

    #[tokio::test]
    async fn test_offboard_nonexistent() {
        let belt = ApiBelt::new();
        let result = belt.offboard("nope").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_health_updates() {
        let belt = ApiBelt::new();
        belt.onboard("https://api.example.com/spec", Some("API"), None, None)
            .await
            .unwrap();

        belt.update_health("API", ApiHealth::Degraded)
            .await
            .unwrap();

        let api = belt.status("API").await.unwrap();
        assert_eq!(api.health, ApiHealth::Degraded);
    }

    #[test]
    fn test_derive_name_from_url() {
        assert_eq!(
            derive_name_from_url("https://api.example.com/v1/openapi.json"),
            "api.example.com"
        );
        assert_eq!(
            derive_name_from_url("https://myapi.io/swagger.yaml"),
            "myapi.io"
        );
        assert_eq!(
            derive_name_from_url("https://example.com/api/openapi.json"),
            "api"
        );
    }

    #[test]
    fn test_api_summary_list() {
        // Test ApiSummary serialization
        let summary = ApiSummary {
            name: "Test".into(),
            tool_count: 5,
            health: ApiHealth::Healthy,
            auth_type: "oidc".into(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("oidc"));
    }
}
