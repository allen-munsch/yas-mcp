//! AI Catalog Generator
//!
//! Auto-generates AI Catalog entries from the tool registry.
//! Serves `/.well-known/ai-catalog.json` for domain-based discovery.
//!
//! The AI Catalog is a cross-protocol discovery standard that lets
//! MCP servers, A2A agents, and other AI artifacts be discovered
//! from a single well-known URL.

use crate::internal::config::AppConfig;
use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::requester::types::RouteConfig;
use serde::{Deserialize, Serialize};

/// A complete AI Catalog document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCatalog {
    #[serde(rename = "mediaType", default = "default_catalog_media_type")]
    pub media_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<CatalogEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<CatalogPublisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

fn default_catalog_media_type() -> String {
    "application/vnd.ai.catalog+json".into()
}

/// A single entry in the catalog (one API surface)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub artifact: ArtifactRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<CatalogPublisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<CatalogDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Reference to an AI artifact (MCP endpoint, A2A agent card, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub url: String,
}

/// Publisher information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPublisher {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Documentation reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogDoc {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Generator that creates AI Catalog entries from the tool registry
pub struct CatalogGenerator;

impl CatalogGenerator {
    /// Generate a full AI Catalog from the current server state.
    ///
    /// Creates one entry for the MCP server and one for the A2A agent card
    /// (if A2A is enabled), plus per-API entries for multi-API instances.
    pub fn generate(
        config: &AppConfig,
        registry: &ToolRegistry,
        _route_configs: &[RouteConfig],
    ) -> AiCatalog {
        let tools = registry.list_metadata();
        let server_url = format!("http://{}:{}", config.server.host, config.server.port);

        let mut entries = Vec::new();

        // Entry 1: MCP Server
        entries.push(CatalogEntry {
            id: format!("{}-mcp", slugify(&config.server.name)),
            media_type: "application/json".into(),
            entry_type: "mcp_server".into(),
            name: config.server.name.clone(),
            description: Some(format!(
                "MCP server proxying REST APIs — {} tools available",
                tools.len()
            )),
            artifact: ArtifactRef {
                media_type: "application/json".into(),
                url: format!("{server_url}/mcp"),
            },
            publisher: config.a2a.as_ref().and_then(|a| {
                a.agent_card_provider.as_ref().map(|p| CatalogPublisher {
                    name: p.organization.clone(),
                    url: p.url.clone(),
                })
            }),
            documentation: None,
            tags: vec!["mcp".into(), "api-proxy".into(), "openapi".into()],
            version: Some(config.server.version.clone()),
        });

        // Entry 2: A2A Agent Card (if enabled)
        if config.a2a.as_ref().map(|a| a.enabled).unwrap_or(false) {
            let a2a_name = config
                .a2a
                .as_ref()
                .and_then(|a| a.agent_card_name.clone())
                .unwrap_or_else(|| format!("{} (A2A)", config.server.name));

            entries.push(CatalogEntry {
                id: format!("{}-a2a", slugify(&config.server.name)),
                media_type: "application/json".into(),
                entry_type: "a2a_agent_card".into(),
                name: a2a_name,
                description: Some(format!(
                    "A2A agent with {} skills for API delegation",
                    tools.len()
                )),
                artifact: ArtifactRef {
                    media_type: "application/json".into(),
                    url: format!("{server_url}/.well-known/agent-card.json"),
                },
                publisher: config.a2a.as_ref().and_then(|a| {
                    a.agent_card_provider.as_ref().map(|p| CatalogPublisher {
                        name: p.organization.clone(),
                        url: p.url.clone(),
                    })
                }),
                documentation: None,
                tags: vec!["a2a".into(), "agent".into(), "delegation".into()],
                version: Some(config.server.version.clone()),
            });
        }

        // Entry 3+: Per-API entries (for multi-API instances — future)
        // Each distinct base path could be a separate entry

        AiCatalog {
            media_type: default_catalog_media_type(),
            name: format!("{} API Catalog", config.server.name),
            description: Some(format!(
                "AI Catalog for {} — {} tools across {} entries",
                config.server.name,
                tools.len(),
                entries.len()
            )),
            entries,
            publisher: config.a2a.as_ref().and_then(|a| {
                a.agent_card_provider.as_ref().map(|p| CatalogPublisher {
                    name: p.organization.clone(),
                    url: p.url.clone(),
                })
            }),
            version: Some(config.server.version.clone()),
        }
    }
}

/// Slugify a name for use in catalog entry IDs
fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::config::{A2aConfig, AgentCardProviderConfig, ServerConfig};
    use crate::internal::mcp::registry::RegisteredTool;
    use std::sync::Arc;

    fn make_test_registry() -> (Arc<ToolRegistry>, Vec<RouteConfig>) {
        let registry = Arc::new(ToolRegistry::new());
        let tool =
            rmcp::model::Tool::new("get_users", "Get users", Arc::new(serde_json::Map::new()));
        let executor: crate::internal::server::tool::handler::ToolExecutor =
            Arc::new(|_| Box::pin(async { Ok(rmcp::model::CallToolResult::success(vec![])) }));
        registry.register(
            "get_users".into(),
            RegisteredTool {
                metadata: tool,
                executor,
            },
        );
        let route = RouteConfig {
            path: "/users".into(),
            method: "GET".into(),
            description: "Get users".into(),
            ..Default::default()
        };
        (registry, vec![route])
    }

    #[test]
    fn test_generate_basic_catalog() {
        let config = AppConfig {
            server: ServerConfig {
                name: "test-server".into(),
                version: "1.0".into(),
                host: "localhost".into(),
                port: 3000,
                ..Default::default()
            },
            ..AppConfig::test_default()
        };
        let (registry, routes) = make_test_registry();
        let catalog = CatalogGenerator::generate(&config, &registry, &routes);

        assert_eq!(catalog.name, "test-server API Catalog");
        assert_eq!(catalog.entries.len(), 1); // Only MCP (no A2A)
        assert_eq!(catalog.entries[0].entry_type, "mcp_server");
        assert!(catalog.entries[0].artifact.url.contains("/mcp"));
    }

    #[test]
    fn test_generate_catalog_with_a2a() {
        let config = AppConfig {
            server: ServerConfig {
                name: "full-server".into(),
                version: "2.0".into(),
                host: "0.0.0.0".into(),
                port: 8080,
                ..Default::default()
            },
            a2a: Some(A2aConfig {
                enabled: true,
                agent_card_name: Some("Full A2A Agent".into()),
                agent_card_provider: Some(AgentCardProviderConfig {
                    organization: "Test Org".into(),
                    url: Some("https://test.example.com".into()),
                }),
                ..Default::default()
            }),
            ..AppConfig::test_default()
        };
        let (registry, routes) = make_test_registry();
        let catalog = CatalogGenerator::generate(&config, &registry, &routes);

        assert_eq!(catalog.entries.len(), 2);
        assert_eq!(catalog.entries[0].entry_type, "mcp_server");
        assert_eq!(catalog.entries[1].entry_type, "a2a_agent_card");
        assert!(catalog.entries[1].artifact.url.contains("agent-card.json"));
        assert!(catalog.publisher.is_some());
    }

    #[test]
    fn test_catalog_json_serialization() {
        let config = AppConfig::test_default();
        let config = AppConfig {
            server: ServerConfig {
                name: "json-test".into(),
                version: "1.0".into(),
                ..Default::default()
            },
            ..config
        };
        let (registry, routes) = make_test_registry();
        let catalog = CatalogGenerator::generate(&config, &registry, &routes);

        let json = serde_json::to_string_pretty(&catalog).unwrap();
        assert!(json.contains("mediaType"));
        assert!(json.contains("mcp_server"));
        assert!(json.contains("/mcp"));
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("My Server"), "my_server");
        assert_eq!(slugify("test-server"), "test-server");
        assert_eq!(slugify("API/Proxy v2.0"), "api_proxy_v2_0");
    }

    #[test]
    fn test_empty_registry() {
        let config = AppConfig::test_default();
        let registry = Arc::new(ToolRegistry::new());
        let routes = vec![];
        let catalog = CatalogGenerator::generate(&config, &registry, &routes);

        assert_eq!(catalog.entries.len(), 1);
        assert!(
            catalog.entries[0]
                .description
                .as_ref()
                .unwrap()
                .contains("0 tools")
        );
    }
}
