use tracing::{error, info};
use yas_mcp::cli::{build_cli, parse_config};
use yas_mcp::internal::server::create_server;
use yas_mcp::internal::config::ServerMode;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matches = build_cli().get_matches();

    // --demo: skip all config, use built-in spec + mock mode
    if matches.get_flag("demo") {
        return run_demo().await;
    }

    let dry_run = matches.get_flag("dry-run");

    // --dry-run: requires --swagger-file, parses, lists tools, exits
    let config = match parse_config(&matches) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = yas_mcp::internal::logger::init_logger(&config.logging) {
        eprintln!("Failed to initialize logger: {}", e);
        std::process::exit(1);
    }

    info!("Starting OpenAPI MCP Server");
    info!("Version: {}", yas_mcp::internal::config::get_version_info());
    info!("Mode: {:?}", config.server.mode);
    info!("OpenAPI file: {}", config.swagger_file);

    // Initialize metrics before anything that uses them
    yas_mcp::internal::telemetry::Metrics::init(&config.server.version);

    if let Some(adjustments_file) = &config.adjustments_file {
        info!("Adjustments file: {}", adjustments_file);
    }

    let server = match create_server(config).await {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            std::process::exit(1);
        }
    };

    // Parse the OpenAPI spec and register tools
    server.setup_tools().await.map_err(|e| {
        error!("Failed to setup tools: {}", e);
    }).ok();

    let tool_count = server.tool_count();
    info!("Server initialized with {} tools", tool_count);

    if dry_run {
        println!();
        println!("  Dry Run — {} tools would be created", tool_count);
        println!();
        let tool_handler = server.tool_handler.lock().await;
        for tool in tool_handler.list_tool_metadata() {
            let desc = tool.description.as_deref().unwrap_or("(no description)");
            println!("  {:<50} {}", tool.name, desc);
        }
        println!();
        println!("  Config looks good. Remove --dry-run to start the server.");
        return Ok(());
    }

    if let Err(e) = server.start_with_graceful_shutdown().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    info!("Server shutdown complete");
    Ok(())
}

async fn run_demo() -> anyhow::Result<()> {
    println!();
    println!("  yas-mcp DEMO — built-in API + mock data, zero config");
    println!();

    let dir = std::env::temp_dir();
    let spec_path = dir.join("yas-mcp-demo-spec.yaml");
    std::fs::write(&spec_path, yas_mcp::internal::parser::demo_spec::DEMO_SPEC)?;
    let spec_path_str = spec_path.to_str().unwrap().to_string();

    let config = yas_mcp::internal::config::AppConfig {
        server: yas_mcp::internal::config::ServerConfig {
            mode: ServerMode::Http,
            host: "0.0.0.0".into(),
            port: 3002,
            name: "yas-mcp Demo".into(),
            version: yas_mcp::internal::config::VERSION.into(),
            ..Default::default()
        },
        endpoint: yas_mcp::internal::config::EndpointConfig {
            mock: true,
            ..Default::default()
        },
        a2a: Some(yas_mcp::internal::config::A2aConfig {
            enabled: true,
            agent_card_name: Some("yas-mcp Demo Agent".into()),
            streaming_enabled: true,
            ..Default::default()
        }),
        swagger_file: spec_path_str.clone(),
        logging: yas_mcp::internal::config::LoggingConfig {
            level: "info".into(),
            format: "compact".into(),
            color: true,
            ..Default::default()
        },
        ..Default::default()
    };

    yas_mcp::internal::logger::init_logger(&config.logging)?;

    let server = create_server(config).await?;
    let tool_count = server.tool_count();

    println!("  {} tools ready (mock mode)", tool_count);
    println!();
    println!("  Endpoints:");
    println!("    MCP:      http://localhost:3002/mcp");
    println!("    Agent:    http://localhost:3002/.well-known/agent-card.json");
    println!("    Catalog:  http://localhost:3002/.well-known/ai-catalog.json");
    println!("    Metrics:  http://localhost:3002/metrics");
    println!();
    println!("  Try: curl -s -X POST http://localhost:3002/mcp \\");
    println!("    -H content-type:application/json \\");
    println!("    -d '{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{{}}}}' | jq .result.tools[].name");
    println!();

    server.start_with_graceful_shutdown().await?;

    let _ = std::fs::remove_file(&spec_path);
    Ok(())
}
