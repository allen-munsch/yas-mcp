use tracing::{error, info};
use yas_mcp::cli::{build_cli, parse_config};
use yas_mcp::internal::server::create_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments first
    let matches = build_cli().get_matches();
    let config = match parse_config(&matches) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    if let Err(e) = yas_mcp::internal::logger::init_logger(&config.logging) {
        eprintln!("Failed to initialize logger: {}", e);
        std::process::exit(1);
    }

    info!("Starting OpenAPI MCP Server");
    info!("Version: {}", yas_mcp::internal::config::get_version_info());
    info!("Mode: {:?}", config.server.mode);
    info!("OpenAPI file: {}", config.swagger_file);

    if let Some(adjustments_file) = &config.adjustments_file {
        info!("Adjustments file: {}", adjustments_file);
    }

    // Create and start server - this now includes tool setup
    let server = match create_server(config).await {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            std::process::exit(1);
        }
    };

    info!("Server initialized with {} tools", server.tool_count());

    // Start server with graceful shutdown
    if let Err(e) = server.start_with_graceful_shutdown().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    info!("Server shutdown complete");
    Ok(())
}