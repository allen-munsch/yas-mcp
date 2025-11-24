use clap::{Arg, Command};
use crate::internal::config::config::{ServerMode, AppConfig};

pub fn build_cli() -> Command {
    // Leak the version string to get a 'static lifetime
    let version: &'static str = Box::leak(
        crate::internal::config::get_version_info().into_boxed_str()
    );
    
    Command::new("yas-mcp")
        .version(version)
        .about("OpenAPI to MCP Server Generator")
        .arg(
            Arg::new("mode")
                .long("mode")
                .value_parser(["stdio", "sse", "http"])
                .default_value("stdio")
                .help("Server mode (stdio|sse|http)")
        )
        .arg(
            Arg::new("swagger-file")
                .long("swagger-file")
                .required(true)
                .help("Path to the OpenAPI/Swagger file")
        )
        .arg(
            Arg::new("adjustments-file")
                .long("adjustments-file")
                .help("Path to the adjustments file")
        )
        .arg(
            Arg::new("config")
                .long("config")
                .help("Path to config file (default: ./config.yaml, /etc/yas-mcp/config.yaml)")
        )
        .arg(
            Arg::new("host")
                .long("host")
                .default_value("127.0.0.1")
                .help("Server host (for http and sse modes)")
        )
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .value_parser(clap::value_parser!(u16))
                .default_value("3000")
                .help("Server port (for http and sse modes)")
        )
        .arg(
            Arg::new("endpoint")
                .long("endpoint")
                .short('e')
                .help("API endpoint base URL for making requests (e.g., http://localhost:8080)")
        )
}

pub fn parse_config(matches: &clap::ArgMatches) -> anyhow::Result<AppConfig> {
    let swagger_file = matches.get_one::<String>("swagger-file")
        .expect("swagger-file is required")
        .to_string();
    
    let adjustments_file = matches.get_one::<String>("adjustments-file")
        .map(|s| s.to_string());
    
    let mode = match matches.get_one::<String>("mode").map(|s| s.as_str()) {
        Some("sse") => ServerMode::Sse,
        Some("http") => ServerMode::Http,
        Some("stdio") | None => ServerMode::Stdio,
        _ => ServerMode::Stdio,
    };
    
    let host = matches.get_one::<String>("host")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    
    let port = matches.get_one::<u16>("port")
        .copied()
        .unwrap_or(3000);
    
    let endpoint_url = matches.get_one::<String>("endpoint")
        .map(|s| s.to_string());
    
    // Try to load from config file first, fall back to CLI args
    match AppConfig::load() {
        Ok(mut config) => {
            // Override with CLI values
            config.swagger_file = swagger_file;
            config.adjustments_file = adjustments_file;
            config.server.mode = mode;
            config.server.host = host;
            config.server.port = port;
            
            // Override endpoint base_url if provided via CLI
            if let Some(url) = endpoint_url {
                config.endpoint.base_url = url;
            }
            
            Ok(config)
        }
        Err(_) => {
            // If config file loading fails, use CLI args only
            let mut config = AppConfig::from_args(swagger_file, adjustments_file, Some(mode));
            config.server.host = host;
            config.server.port = port;
            
            // Set endpoint base_url if provided
            if let Some(url) = endpoint_url {
                config.endpoint.base_url = url;
            }
            
            Ok(config)
        }
    }
}