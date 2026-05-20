use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Version information from build script - using option_env! for safety
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Get version information
pub fn get_version_info() -> String {
    let build_timestamp = option_env!("VERGEN_BUILD_TIMESTAMP").unwrap_or("unknown");
    let build_date = option_env!("VERGEN_BUILD_DATE").unwrap_or("unknown");
    let git_describe = option_env!("VERGEN_GIT_DESCRIBE").unwrap_or("unknown");
    let git_commit_hash = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let git_commit_date = option_env!("VERGEN_GIT_COMMIT_DATE").unwrap_or("unknown");
    let git_branch = option_env!("VERGEN_GIT_BRANCH").unwrap_or("unknown");
    let rustc_semver = option_env!("VERGEN_RUSTC_SEMVER").unwrap_or("unknown");
    let cargo_target_triple = option_env!("VERGEN_CARGO_TARGET_TRIPLE").unwrap_or("unknown");

    format!(
        "yas-mcp version {} ({})\n\
         Built: {} ({})\n\
         Git: {} on {} ({})\n\
         Rust: {}\n\
         Target: {}",
        VERSION,
        git_describe,
        build_date,
        build_timestamp,
        git_commit_hash,
        git_branch,
        git_commit_date,
        rustc_semver,
        cargo_target_triple
    )
}

/// AuthType represents the type of authentication to use
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum AuthType {
    #[serde(rename = "none")]
    #[default]
    None,
    #[serde(rename = "basic")]
    Basic,
    #[serde(rename = "bearer")]
    Bearer,
    #[serde(rename = "api_key")]
    ApiKey,
    #[serde(rename = "oauth2")]
    OAuth2,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointConfig {
    pub base_url: String,
    #[serde(default)]
    pub auth_type: AuthType,
    #[serde(default)]
    pub auth_config: HashMap<String, String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Mock mode — generate responses from OpenAPI schemas instead of calling upstream
    #[serde(default)]
    pub mock: bool,
    /// Record API responses to disk (experimental)
    #[cfg(feature = "record-replay")]
    #[serde(default)]
    pub record: bool,
    /// Replay API responses from disk (experimental)
    #[cfg(feature = "record-replay")]
    #[serde(default)]
    pub replay: bool,
    /// Directory for recordings
    #[cfg(feature = "record-replay")]
    #[serde(default = "default_recordings_dir")]
    pub recordings_dir: String,
}

#[cfg(feature = "record-replay")]
fn default_recordings_dir() -> String {
    "recordings".into()
}

/// ServerMode represents the server operation mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ServerMode {
    #[serde(rename = "sse")]
    Sse,
    #[serde(rename = "stdio")]
    #[default]
    Stdio,
    #[serde(rename = "http")]
    Http,
    /// gRPC mode — production transport
    #[serde(rename = "grpc")]
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_timeout")]
    pub timeout: String,
    #[serde(default)]
    pub mode: ServerMode,
    #[serde(default = "default_name")]
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    /// gRPC port (only used with --mode grpc)
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
}

fn default_grpc_port() -> u16 {
    50051
}

fn default_port() -> u16 {
    3000
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_timeout() -> String {
    "30s".to_string()
}
fn default_name() -> String {
    "yas-mcp".to_string()
}
fn default_version() -> String {
    VERSION.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_true")]
    pub color: bool,
    #[serde(default)]
    pub disable_stacktrace: bool,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub append_to_file: bool,
    #[serde(default)]
    pub disable_console: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_format() -> String {
    "compact".to_string()
}
fn default_true() -> bool {
    true
}

/// WIMSE workload identity configuration for token exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WimseConfig {
    /// Enable WIMSE token exchange endpoint at /api/auth/exchange
    #[serde(default)]
    pub enabled: bool,

    /// Trust domain for identity validation (e.g., "weft.allen.local")
    pub trust_domain: String,

    /// HMAC signing key (base64-encoded) shared with the Weft control plane
    pub signing_key: String,

    /// Optional: allow audiences not in the standard weft: namespace
    #[serde(default)]
    pub allow_custom_audiences: bool,
}

// Add to AppConfig struct:
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub endpoint: EndpointConfig,
    pub swagger_file: String,
    pub adjustments_file: Option<String>,
    pub oauth: Option<OAuthConfig>,
    #[serde(default)]
    pub a2a: Option<A2aConfig>,
    #[serde(default)]
    pub auth: Option<AuthMiddlewareConfig>,
    #[serde(default)]
    pub secrets: Option<SecretsConfig>,
    #[serde(default)]
    pub cache: Option<crate::internal::control::CacheConfig>,
    /// WIMSE workload identity token exchange (optional)
    #[serde(default)]
    pub wimse: Option<WimseConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub enabled: bool,
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub allow_origins: Vec<String>,

    // OIDC Discovery: just set this and everything is auto-configured
    #[serde(default)]
    pub issuer_url: Option<String>,
    /// Cache TTL for the OIDC discovery document (seconds, default: 3600)
    #[serde(default = "default_oidc_cache_ttl")]
    pub oidc_cache_ttl: u64,

    // For generic/manual providers (ignored if issuer_url is set)
    pub auth_url: Option<String>,
    pub token_url: Option<String>,
    pub user_info_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub extra_params: Option<HashMap<String, String>>,
}

fn default_oidc_cache_ttl() -> u64 {
    3600
}

/// A2A Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct A2aConfig {
    pub enabled: bool,
    #[serde(default)]
    pub agent_card_name: Option<String>,
    #[serde(default)]
    pub agent_card_description: Option<String>,
    #[serde(default)]
    pub agent_card_url: Option<String>,
    #[serde(default)]
    pub agent_card_documentation: Option<String>,
    #[serde(default)]
    pub agent_card_provider: Option<AgentCardProviderConfig>,
    #[serde(default = "default_task_ttl")]
    pub task_ttl: u64,
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,
    #[serde(default = "default_true")]
    pub streaming_enabled: bool,
    #[serde(default)]
    pub push_notifications_enabled: bool,
}

fn default_task_ttl() -> u64 {
    3600
}
fn default_max_concurrent_tasks() -> usize {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardProviderConfig {
    pub organization: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// Auth middleware configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMiddlewareConfig {
    #[serde(default)]
    pub middleware_chain: Vec<AuthProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String,
    #[serde(default)]
    pub route_filter: Option<String>,
    #[serde(default)]
    pub config: HashMap<String, String>,
}

/// Secrets management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Additional secret backends to register (env and file are always available)
    #[serde(default)]
    pub backends: Vec<SecretBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretBackendConfig {
    /// Backend type: "vault", "aws-secretsmanager"
    #[serde(rename = "type")]
    pub backend_type: String,
    /// Backend-specific configuration
    #[serde(default)]
    pub config: HashMap<String, String>,
}

impl AppConfig {
    /// Create a minimal config for testing
    #[cfg(test)]
    pub fn test_default() -> Self {
        Self {
            swagger_file: "examples/todo-app/openapi.yaml".into(),
            ..Default::default()
        }
    }

    pub fn load(app_config_path: Option<&str>) -> Result<Self, ConfigError> {
        // Strip known extensions — File::with_name auto-appends .yaml/.json/etc.
        // so passing "config.yaml" results in "config.yaml.yaml" (not found).
        let config_name = match app_config_path {
            Some(p) if p.ends_with(".yaml") || p.ends_with(".yml")
                       || p.ends_with(".json") || p.ends_with(".toml") => {
                &p[..p.rfind('.').unwrap()]
            }
            Some(p) => p,
            None => "config",
        };

        let config_builder = Config::builder()
            // Start with default values
            .set_default("server.port", 3000)?
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.timeout", "30s")?
            .set_default("server.mode", "stdio")?
            .set_default("server.name", "yas-mcp")?
            .set_default("server.version", VERSION)?
            .set_default("logging.level", "info")?
            .set_default("logging.format", "compact")?
            .set_default("logging.color", true)?
            .set_default("endpoint.auth_type", false)?
            // Load the explicitly specified config file
            .add_source(File::with_name(config_name).required(false));

        // Only auto-load fallback configs when --config was NOT explicitly provided
        let config_builder = if app_config_path.is_none() {
            config_builder
                .add_source(File::with_name("config").required(false))
                .add_source(File::with_name("/etc/yas-mcp/config").required(false))
                .add_source(File::with_name("/config/config").required(false))
        } else {
            config_builder
        };

        let config_builder = config_builder
            // Environment variables
            .add_source(
                config::Environment::with_prefix("YAS_MCP")
                    .try_parsing(true)
                    .separator("_")
                    .list_separator(" "),
            );

        let config = config_builder.build()?;
        let mut app_config: AppConfig = config.try_deserialize()?;

        // Validate required fields
        if app_config.swagger_file.is_empty() {
            return Err(ConfigError::Message("swagger file is required".to_string()));
        }

        // Process scopes if they're provided as space-separated string
        if let Some(oauth) = &mut app_config.oauth {
            if oauth.scopes.len() == 1 {
                let single_scope = &oauth.scopes[0];
                if single_scope.contains(' ') {
                    oauth.scopes = single_scope
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect();
                }
            }
        }

        Ok(app_config)
    }

    pub fn from_args(
        swagger_file: String,
        adjustments_file: Option<String>,
        mode: Option<ServerMode>,
    ) -> Self {
        Self {
            swagger_file,
            adjustments_file,
            server: ServerConfig {
                mode: mode.unwrap_or_default(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn load_from_args(matches: &clap::ArgMatches) -> Result<Self, ConfigError> {
        // Extract the config path from CLI matches first
        let config_path = matches.get_one::<String>("config").map(|s| s.as_str());
        // Pass it into the newly updated load function
        let mut config = Self::load(config_path)?;

        // Override with CLI args if provided
        if let Some(swagger_file) = matches.get_one::<String>("swagger-file") {
            config.swagger_file = swagger_file.clone();
        }

        if let Some(adjustments_file) = matches.get_one::<String>("adjustments-file") {
            config.adjustments_file = Some(adjustments_file.clone());
        }

        if let Some(mode) = matches.get_one::<String>("mode") {
            config.server.mode = match mode.as_str() {
                "sse" => ServerMode::Sse,
                "http" => ServerMode::Http,
                "stdio" => ServerMode::Stdio,
                _ => ServerMode::Stdio, // Handle unknown modes explicitly
            };
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = AppConfig::default();
        // Derive(Default) gives Rust-native defaults, not serde defaults
        assert_eq!(config.server.port, 0); // u16::default()
        assert_eq!(config.server.host, ""); // String::default()
        assert_eq!(config.server.mode, ServerMode::Stdio); // #[default]
        assert_eq!(config.server.name, "");
        assert_eq!(config.logging.level, "");
        assert_eq!(config.endpoint.auth_type, AuthType::None);
        assert!(config.swagger_file.is_empty());
    }

    #[test]
    fn test_test_default_has_swagger_file() {
        let config = AppConfig::test_default();
        assert!(!config.swagger_file.is_empty());
    }

    #[test]
    fn test_server_mode_serialization() {
        let modes = vec![
            ("sse", ServerMode::Sse),
            ("stdio", ServerMode::Stdio),
            ("http", ServerMode::Http),
        ];

        for (name, mode) in modes {
            let config = AppConfig {
                server: ServerConfig {
                    mode,
                    ..Default::default()
                },
                ..AppConfig::test_default()
            };
            let yaml = serde_yaml::to_string(&config).unwrap();
            assert!(yaml.contains(name), "YAML should contain mode '{name}'");
        }
    }

    #[test]
    fn test_a2a_config_defaults() {
        let config = A2aConfig {
            enabled: true,
            agent_card_name: None,
            agent_card_description: None,
            agent_card_url: None,
            agent_card_documentation: None,
            agent_card_provider: None,
            task_ttl: 3600,
            max_concurrent_tasks: 100,
            streaming_enabled: true,
            push_notifications_enabled: false,
        };

        assert_eq!(config.task_ttl, 3600);
        assert_eq!(config.max_concurrent_tasks, 100);
        assert!(config.streaming_enabled);
        assert!(!config.push_notifications_enabled);
    }

    #[test]
    fn test_auth_middleware_config() {
        let config = AuthMiddlewareConfig {
            middleware_chain: vec![AuthProviderConfig {
                provider_type: "bearer_token".into(),
                route_filter: Some("/api/**".into()),
                config: {
                    let mut m = HashMap::new();
                    m.insert("token".into(), "secret123".into());
                    m
                },
            }],
        };

        assert_eq!(config.middleware_chain.len(), 1);
        assert_eq!(config.middleware_chain[0].provider_type, "bearer_token");
        assert_eq!(
            config.middleware_chain[0].config.get("token").unwrap(),
            "secret123"
        );
    }

    #[test]
    fn test_secrets_config() {
        let config = SecretsConfig { backends: vec![] };
        assert!(config.backends.is_empty());

        let config = SecretsConfig {
            backends: vec![SecretBackendConfig {
                backend_type: "vault".into(),
                config: {
                    let mut m = HashMap::new();
                    m.insert("address".into(), "http://vault:8200".into());
                    m
                },
            }],
        };
        assert_eq!(config.backends.len(), 1);
        assert_eq!(config.backends[0].backend_type, "vault");
    }

    #[test]
    fn test_auth_type_deserialization() {
        let none: AuthType = serde_json::from_str("\"none\"").unwrap();
        assert_eq!(none, AuthType::None);

        let bearer: AuthType = serde_json::from_str("\"bearer\"").unwrap();
        assert_eq!(bearer, AuthType::Bearer);

        let oauth2: AuthType = serde_json::from_str("\"oauth2\"").unwrap();
        assert_eq!(oauth2, AuthType::OAuth2);
    }

    #[test]
    fn test_version_string() {
        let version = get_version_info();
        assert!(version.contains(VERSION));
        assert!(version.contains("yas-mcp"));
    }

    #[test]
    fn test_config_round_trip() {
        // Build → serialize → deserialize → compare
        let config = AppConfig {
            server: ServerConfig {
                mode: ServerMode::Http,
                port: 8080,
                host: "0.0.0.0".into(),
                name: "round-trip-test".into(),
                version: "2.0.0".into(),
                ..Default::default()
            },
            swagger_file: "/tmp/test.yaml".into(),
            ..AppConfig::test_default()
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(parsed.server.mode, ServerMode::Http);
        assert_eq!(parsed.server.port, 8080);
        assert_eq!(parsed.server.name, "round-trip-test");
        assert_eq!(parsed.swagger_file, "/tmp/test.yaml");
    }

    #[test]
    fn test_config_round_trip_with_a2a() {
        let mut config = AppConfig::test_default();
        config.a2a = Some(A2aConfig {
            enabled: true,
            agent_card_name: Some("Test".into()),
            task_ttl: 7200,
            streaming_enabled: true,
            push_notifications_enabled: false,
            ..Default::default()
        });

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: AppConfig = serde_yaml::from_str(&yaml).unwrap();

        let a2a = parsed.a2a.unwrap();
        assert!(a2a.enabled);
        assert_eq!(a2a.agent_card_name, Some("Test".into()));
        assert_eq!(a2a.task_ttl, 7200);
    }
}
