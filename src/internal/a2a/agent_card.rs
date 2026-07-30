//! Agent Card Generator
//!
//! Auto-generates A2A Agent Cards from the yas-mcp tool registry.
//! No duplication — the tool registry is the single source of truth.

use crate::internal::a2a::types::*;
use crate::internal::config::AppConfig;
use crate::internal::mcp::registry::ToolRegistry;
use crate::internal::requester::types::RouteConfig;

/// Generates an Agent Card from the current tool registry state
pub struct AgentCardGenerator;

impl AgentCardGenerator {
    /// Generate a full Agent Card from tool registry + route configs
    pub fn generate(
        config: &AppConfig,
        registry: &ToolRegistry,
        route_configs: &[RouteConfig],
    ) -> AgentCard {
        let tools = registry.list_metadata();
        let a2a_config = config.a2a.as_ref();

        // Map MCP tools → A2A skills
        let skills: Vec<AgentSkill> = tools
            .iter()
            .zip(route_configs.iter())
            .map(|(tool, route)| Self::tool_to_skill(tool, route))
            .collect();

        let name = a2a_config
            .and_then(|c| c.agent_card_name.clone())
            .unwrap_or_else(|| format!("yas-mcp / {}", config.server.name));

        let description = a2a_config
            .and_then(|c| c.agent_card_description.clone())
            .unwrap_or_else(|| {
                format!(
                    "MCP server proxying REST APIs — {} tools available",
                    skills.len()
                )
            });

        let url = a2a_config
            .and_then(|c| c.agent_card_url.clone())
            .unwrap_or_else(|| format!("http://{}:{}", config.server.host, config.server.port));

        let capabilities = AgentCapabilities {
            streaming: a2a_config.map(|c| c.streaming_enabled).unwrap_or(true),
            push_notifications: a2a_config
                .map(|c| c.push_notifications_enabled)
                .unwrap_or(false),
            state_transition_history: true,
            extensions: Vec::new(),
        };

        let provider = a2a_config
            .and_then(|c| c.agent_card_provider.clone())
            .map(|p| AgentProvider {
                organization: p.organization,
                url: p.url,
            });

        AgentCard {
            name,
            description,
            url,
            provider,
            version: config.server.version.clone(),
            capabilities,
            skills,
            default_input_modes: vec![
                "text".into(),
                "text/plain".into(),
                "application/json".into(),
            ],
            default_output_modes: vec![
                "text".into(),
                "text/plain".into(),
                "application/json".into(),
            ],
            documentation: a2a_config.and_then(|c| c.agent_card_documentation.clone()),
            icons: None,
        }
    }

    /// Map a single MCP tool to an A2A skill
    fn tool_to_skill(tool: &rmcp::model::Tool, route: &RouteConfig) -> AgentSkill {
        let tags = Self::extract_tags(&route.path, &route.method);

        let examples = Self::generate_examples(&tool.input_schema);

        AgentSkill {
            id: tool.name.to_string(),
            name: tool
                .title
                .clone()
                .unwrap_or_else(|| tool.name.to_string()),
            description: Some(route.description.clone()),
            tags,
            examples,
            input_modes: vec!["application/json".into()],
            output_modes: vec!["application/json".into()],
        }
    }

    /// Extract semantic tags from path and method
    fn extract_tags(path: &str, method: &str) -> Vec<String> {
        let mut tags = Vec::new();

        // Method-based tags
        match method {
            "GET" => tags.push("read".into()),
            "POST" => tags.push("create".into()),
            "PUT" | "PATCH" => tags.push("update".into()),
            "DELETE" => tags.push("delete".into()),
            _ => {}
        }

        // Path-based tags: extract resource names
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            let clean = segment
                .trim_start_matches('{')
                .trim_end_matches('}')
                .replace('-', "_")
                .replace('_', " ");
            if !clean.is_empty() && !clean.starts_with('{') {
                tags.push(clean);
            }
        }

        tags
    }

    /// Generate human-readable usage examples from the input schema
    fn generate_examples(input_schema: &std::sync::Arc<rmcp::model::JsonObject>) -> Vec<String> {
        let mut examples = Vec::new();

        // input_schema is Arc<Map<String, Value>> — extract property names
        if let Some(props) = input_schema.get("properties").and_then(|v| v.as_object()) {
            let field_names: Vec<&str> = props.keys().map(|k| k.as_str()).collect();

            if !field_names.is_empty() {
                // Check for required fields
                if let Some(required) = input_schema.get("required").and_then(|v| v.as_array()) {
                    let required_fields: Vec<String> = required
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();

                    if !required_fields.is_empty() {
                        examples.push(format!("Required fields: {}", required_fields.join(", ")));
                    }
                }

                // Generate a sample call with first few fields
                let sample_args: Vec<String> = field_names
                    .iter()
                    .take(3)
                    .map(|f| format!("{}: <value>", f))
                    .collect();

                if !sample_args.is_empty() {
                    examples.push(format!("Call with: {}", sample_args.join(", ")));
                }
            }
        }

        if examples.is_empty() {
            examples.push("No required parameters".into());
        }

        examples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::config::A2aConfig;
    use crate::internal::mcp::registry::{RegisteredTool, ToolRegistry};
    use crate::internal::requester::types::MethodConfig;
    use rmcp::model::Tool;
    use std::sync::Arc;

    fn make_test_config() -> AppConfig {
        AppConfig {
            server: crate::internal::config::ServerConfig {
                name: "test-server".into(),
                version: "0.1.0".into(),
                host: "127.0.0.1".into(),
                port: 3000,
                ..Default::default()
            },
            a2a: Some(A2aConfig {
                enabled: true,
                agent_card_name: Some("Test Agent Card".into()),
                agent_card_description: Some("Test description".into()),
                agent_card_url: Some("https://test.example.com".into()),
                agent_card_provider: Some(crate::internal::config::AgentCardProviderConfig {
                    organization: "Test Org".into(),
                    url: Some("https://example.com".into()),
                }),
                agent_card_documentation: None,
                task_ttl: 3600,
                max_concurrent_tasks: 100,
                streaming_enabled: true,
                push_notifications_enabled: false,
            }),
            ..Default::default()
        }
    }

    fn make_test_registry() -> (Arc<ToolRegistry>, Vec<RouteConfig>) {
        let registry = Arc::new(ToolRegistry::new());

        let tool = Tool::new(
            "get_users",
            "Get all users",
            Arc::new({
                let mut map = serde_json::Map::new();
                map.insert("type".into(), serde_json::json!("object"));
                map.insert(
                    "properties".into(),
                    serde_json::json!({
                        "page": {"type": "integer", "description": "Page number"}
                    }),
                );
                map.insert("required".into(), serde_json::json!(["page"]));
                map
            }),
        )
        .with_title("List Users");

        let executor: crate::internal::server::tool::handler::ToolExecutor = Arc::new(|_req| {
            Box::pin(async {
                Ok(rmcp::model::CallToolResult::success(vec![]))
            })
        });

        registry.register(
            "get_users".into(),
            RegisteredTool {
                metadata: tool.clone(),
                executor,
            },
        );

        let route_config = RouteConfig {
            path: "/users".into(),
            method: "GET".into(),
            description: "Retrieve all users with pagination".into(),
            method_config: MethodConfig {
                query_params: vec!["page".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        (registry, vec![route_config])
    }

    #[test]
    fn test_extract_tags() {
        let tags = AgentCardGenerator::extract_tags("/users/{userId}/posts", "POST");
        assert!(!tags.contains(&"read".to_string()), "POST should not have 'read' tag");
        assert!(tags.contains(&"create".to_string()));
        assert!(tags.contains(&"users".to_string()));
        assert!(tags.contains(&"userId".to_string()));
        assert!(tags.contains(&"posts".to_string()));
    }

    #[test]
    fn test_extract_tags_get() {
        let tags = AgentCardGenerator::extract_tags("/api/projects", "GET");
        assert!(tags.contains(&"read".to_string()));
        assert!(tags.contains(&"api".to_string()));
        assert!(tags.contains(&"projects".to_string()));
    }

    #[test]
    fn test_generate_agent_card() {
        let config = make_test_config();
        let (registry, routes) = make_test_registry();

        let card = AgentCardGenerator::generate(&config, &registry, &routes);

        assert_eq!(card.name, "Test Agent Card");
        assert_eq!(card.description, "Test description");
        assert_eq!(card.url, "https://test.example.com");
        assert_eq!(card.version, "0.1.0");
        assert_eq!(card.skills.len(), 1);
        assert_eq!(card.skills[0].id, "get_users");
        assert_eq!(card.skills[0].name, "List Users");
        assert!(card.capabilities.streaming);
        assert!(!card.capabilities.push_notifications);
        assert!(card.provider.is_some());
    }

    #[test]
    fn test_generate_agent_card_no_config() {
        let config = AppConfig {
            server: crate::internal::config::ServerConfig {
                name: "minimal".into(),
                version: "1.0.0".into(),
                host: "0.0.0.0".into(),
                port: 8080,
                ..Default::default()
            },
            ..Default::default()
        };
        let (registry, routes) = make_test_registry();

        let card = AgentCardGenerator::generate(&config, &registry, &routes);

        assert_eq!(card.name, "yas-mcp / minimal");
        assert_eq!(card.url, "http://0.0.0.0:8080");
        assert_eq!(card.skills.len(), 1);
    }

    #[test]
    fn test_generate_examples() {
        use std::sync::Arc;
        use serde_json::json;

        let input_schema = Arc::new({
            let mut map = serde_json::Map::new();
            map.insert("type".into(), json!("object"));
            map.insert(
                "properties".into(),
                json!({
                    "name": {"type": "string", "description": "User name"},
                    "email": {"type": "string", "description": "Email address"}
                }),
            );
            map.insert("required".into(), json!(["name"]));
            map
        });

        let examples = AgentCardGenerator::generate_examples(&input_schema);
        assert!(!examples.is_empty());
        assert!(
            examples.iter().any(|e| e.contains("name: <value>")),
            "Should contain call example: {examples:?}"
        );
    }
}
