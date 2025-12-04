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

// Add to AppConfig struct:
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub endpoint: EndpointConfig,
    pub swagger_file: String,
    pub adjustments_file: Option<String>,
    pub oauth: Option<OAuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub enabled: bool,
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub allow_origins: Vec<String>,

    // For generic providers
    pub auth_url: Option<String>,
    pub token_url: Option<String>,
    pub user_info_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub extra_params: Option<HashMap<String, String>>,
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
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
            .set_default("endpoint.auth_type", "none")?
            // Load config files in order of precedence
            .add_source(File::with_name("config").required(false))
            .add_source(File::with_name("/etc/yas-mcp/config").required(false))
            .add_source(File::with_name("/config/config").required(false))
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
        let mut config = Self::load()?;

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








