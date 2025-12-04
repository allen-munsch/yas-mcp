use clap::{Arg, Command};
use yas_mcp::internal::config;

fn main() -> anyhow::Result<()> {
    // Leak the version string to get a 'static lifetime
    let version: &'static str = Box::leak(config::get_version_info().into_boxed_str());

    let matches = Command::new("mcp-config-builder")
        .version(version)
        .about("A tool to build MCP config from Swagger/OpenAPI definitions")
        .arg(
            Arg::new("swagger-file")
                .long("swagger-file")
                .required(true)
                .help("Path to the Swagger/OpenAPI file"),
        )
        .arg(
            Arg::new("adjustments-file")
                .long("adjustments-file")
                .help("Path to the MCP adjustments file"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .short('v')
                .action(clap::ArgAction::SetTrue)
                .help("Show version information"),
        )
        .get_matches();

    // Handle version flag
    if matches.get_flag("version") {
        println!("{}", version);
        return Ok(());
    }

    let swagger_file = matches
        .get_one::<String>("swagger-file")
        .expect("swagger-file is required");

    let adjustments_file = matches.get_one::<String>("adjustments-file");

    println!("MCP Config Builder");
    println!("OpenAPI file: {}", swagger_file);

    if let Some(adj_file) = adjustments_file {
        println!("Adjustments file: {}", adj_file);
    }

    // TODO: Implement TUI for route selection and description editing
    println!("TUI functionality not yet implemented");

    Ok(())
}
