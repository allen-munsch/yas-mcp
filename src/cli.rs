use crate::internal::config::{AppConfig, ServerMode};
use clap::{Arg, Command};

pub fn build_cli() -> Command {
    // Leak the version string to get a 'static lifetime
    let version: &'static str =
        Box::leak(crate::internal::config::get_version_info().into_boxed_str());

    Command::new("yas-mcp")
        .version(version)
        .about("OpenAPI to MCP Server Generator")
        .arg(
            Arg::new("mode")
                .long("mode")
                .value_parser(["stdio", "sse", "http", "grpc"])
                .default_value("stdio")
                .help("Server mode (stdio|sse|http|grpc)"),
        )
        .arg(
            Arg::new("swagger-file")
                .long("swagger-file")
                .required_unless_present_any(["demo", "config"])
                .help("Path to the OpenAPI/Swagger file"),
        )
        .arg(
            Arg::new("adjustments-file")
                .long("adjustments-file")
                .help("Path to the adjustments file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .help("Path to config file (default: ./config.yaml, /etc/yas-mcp/config.yaml)"),
        )
        .arg(
            Arg::new("host")
                .long("host")
                .default_value("127.0.0.1")
                .help("Server host (for http and sse modes)"),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .value_parser(clap::value_parser!(u16))
                .default_value("3000")
                .help("Server port (for http and sse modes)"),
        )
        .arg(
            Arg::new("endpoint")
                .long("endpoint")
                .short('e')
                .help("API endpoint base URL for making requests (e.g., http://localhost:8080)"),
        )
        .arg(
            Arg::new("mock")
                .long("mock")
                .action(clap::ArgAction::SetTrue)
                .help("Mock mode: generate responses from OpenAPI schemas, no upstream needed"),
        )
        .arg(
            Arg::new("grpc-port")
                .long("grpc-port")
                .value_parser(clap::value_parser!(u16))
                .default_value("50051")
                .help("gRPC server port (requires --mode grpc)"),
        )
        .arg(
            Arg::new("record")
                .long("record")
                .action(clap::ArgAction::SetTrue)
                .help(if cfg!(feature = "record-replay") {
                    "[EXPERIMENTAL] Record API responses to disk for later replay"
                } else {
                    "[EXPERIMENTAL] Record API responses (requires --features record-replay)"
                }),
        )
        .arg(
            Arg::new("replay")
                .long("replay")
                .action(clap::ArgAction::SetTrue)
                .help(if cfg!(feature = "record-replay") {
                    "[EXPERIMENTAL] Replay API responses from disk, never call upstream"
                } else {
                    "[EXPERIMENTAL] Replay API responses (requires --features record-replay)"
                }),
        )
        .arg(
            Arg::new("recordings-dir")
                .long("recordings-dir")
                .default_value("recordings")
                .help("Directory for recording/replay files"),
        )
        .arg(
            Arg::new("demo")
                .long("demo")
                .action(clap::ArgAction::SetTrue)
                .help("Demo mode: start with built-in spec + mock data, zero config needed"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue)
                .help("Parse spec and list tools, then exit without starting server"),
        )
}

pub fn parse_config(matches: &clap::ArgMatches) -> anyhow::Result<AppConfig> {
    let swagger_file = matches
        .get_one::<String>("swagger-file")
        .map(|s| s.to_string())
        .unwrap_or_default();

    let adjustments_file = matches
        .get_one::<String>("adjustments-file")
        .map(|s| s.to_string());

    let mode = match matches.get_one::<String>("mode").map(|s| s.as_str()) {
        Some("sse") => ServerMode::Sse,
        Some("http") => ServerMode::Http,
        Some("grpc") => ServerMode::Grpc,
        Some("stdio") | None => ServerMode::Stdio,
        _ => ServerMode::Stdio,
    };

    let host = matches
        .get_one::<String>("host")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let port = matches.get_one::<u16>("port").copied().unwrap_or(3000);

    let endpoint_url = matches.get_one::<String>("endpoint").map(|s| s.to_string());
    let mock_mode = matches.get_flag("mock");
    #[cfg(feature = "record-replay")]
    let record_mode = matches.get_flag("record");
    #[cfg(feature = "record-replay")]
    let replay_mode = matches.get_flag("replay");
    #[cfg(feature = "record-replay")]
    let recordings_dir = matches.get_one::<String>("recordings-dir").cloned();
    let config_arg = matches.get_one::<String>("config").map(|s| s.as_str());
    
    // Try to load from config file first, fall back to CLI args
    match AppConfig::load(config_arg) {
        Ok(mut config) => {
            // Override with CLI values (only if explicitly provided, not defaults)
            if matches.value_source("swagger-file") == Some(clap::parser::ValueSource::CommandLine) {
                config.swagger_file = swagger_file;
            }
            if adjustments_file.is_some() {
                config.adjustments_file = adjustments_file;
            }
            config.server.mode = mode;
            config.server.host = host;
            config.server.port = port;
            if let Some(grpc_port) = matches.get_one::<u16>("grpc-port") {
                config.server.grpc_port = *grpc_port;
            }

            // Override endpoint base_url if provided via CLI
            if let Some(url) = endpoint_url {
                config.endpoint.base_url = url;
            }

            // Mock mode
            if mock_mode {
                config.endpoint.mock = true;
            }

            // Record/replay (experimental)
            #[cfg(feature = "record-replay")]
            {
                if record_mode {
                    config.endpoint.record = true;
                }
                if replay_mode {
                    config.endpoint.replay = true;
                }
                if let Some(dir) = recordings_dir {
                    config.endpoint.recordings_dir = dir;
                }
            }

            Ok(config)
        }
        Err(_) => {
            // If config file loading fails, use CLI args only
            let mut config = AppConfig::from_args(swagger_file, adjustments_file, Some(mode));
            config.server.host = host;
            config.server.port = port;
            if let Some(grpc_port) = matches.get_one::<u16>("grpc-port") {
                config.server.grpc_port = *grpc_port;
            }

            // Set endpoint base_url if provided
            if let Some(url) = endpoint_url {
                config.endpoint.base_url = url;
            }

            if mock_mode {
                config.endpoint.mock = true;
            }

            #[cfg(feature = "record-replay")]
            {
                if record_mode {
                    config.endpoint.record = true;
                }
                if replay_mode {
                    config.endpoint.replay = true;
                }
                if let Some(dir) = recordings_dir {
                    config.endpoint.recordings_dir = dir;
                }
            }

            Ok(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cli() {
        let cmd = build_cli();
        let name = cmd.get_name();
        assert_eq!(name, "yas-mcp");
    }

    #[test]
    fn test_parse_mode_stdio() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(vec![
            "yas-mcp",
            "--swagger-file", "test.yaml",
            "--mode", "stdio",
        ]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_parse_mode_http() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(vec![
            "yas-mcp",
            "--swagger-file", "test.yaml",
            "--mode", "http",
            "--port", "8080",
        ]);
        assert!(matches.is_ok());
        let m = matches.unwrap();
        assert_eq!(m.get_one::<String>("mode").unwrap(), "http");
        assert_eq!(*m.get_one::<u16>("port").unwrap(), 8080);
    }

    #[test]
    fn test_parse_missing_required_swagger() {
        let cmd = build_cli();
        let result = cmd.try_get_matches_from(vec!["yas-mcp"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_no_swagger_ok() {
        // --swagger-file not required when --config is provided
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(vec!["yas-mcp", "--config", "cfg.yaml"]);
        assert!(matches.is_ok());
    }

    #[test]
    fn test_parse_with_all_options() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(vec![
            "yas-mcp",
            "--swagger-file", "spec.yaml",
            "--adjustments-file", "adj.yaml",
            "--config", "cfg.yaml",
            "--mode", "http",
            "--host", "0.0.0.0",
            "--port", "3001",
            "--endpoint", "http://api:8080",
        ]);
        assert!(matches.is_ok());
        let m = matches.unwrap();
        assert_eq!(m.get_one::<String>("swagger-file").unwrap(), "spec.yaml");
        assert_eq!(m.get_one::<String>("adjustments-file").unwrap(), "adj.yaml");
        assert_eq!(m.get_one::<String>("config").unwrap(), "cfg.yaml");
        assert_eq!(m.get_one::<String>("endpoint").unwrap(), "http://api:8080");
    }

    #[test]
    fn test_parse_default_port() {
        let cmd = build_cli();
        let matches = cmd
            .try_get_matches_from(vec!["yas-mcp", "--swagger-file", "test.yaml"])
            .unwrap();
        assert_eq!(*matches.get_one::<u16>("port").unwrap(), 3000);
    }

    #[test]
    fn test_parse_default_mode() {
        let cmd = build_cli();
        let matches = cmd
            .try_get_matches_from(vec!["yas-mcp", "--swagger-file", "test.yaml"])
            .unwrap();
        assert_eq!(matches.get_one::<String>("mode").unwrap(), "stdio");
    }
}