use std::io::Read;
use anyhow::Result;

// Assuming we'll create these modules later
use crate::internal::requester::RouteConfig;

// Use the actual MCP tool type from rmcp crate
pub type McpTool = rmcp::model::Tool;

/// RouteTool combines a route configuration with its corresponding MCP tool
#[derive(Debug, Clone)]
pub struct RouteTool {
    pub route_config: RouteConfig,
    pub tool: rmcp::model::Tool,
}

/// Parser handles parsing of Swagger/OpenAPI specifications
pub trait Parser: Send + Sync {
    /// Init parses a Swagger/OpenAPI specification from a file
    fn init(&mut self, openapi_spec: &str, adjustments_file: Option<&str>) -> Result<()>;
    
    /// ParseReader parses a Swagger/OpenAPI specification from a reader
    /// Note: Removed generic to make trait dyn-compatible
    /// Use Box<dyn Read> instead of generic R: Read
    fn parse_reader(&mut self, reader: Box<dyn Read>) -> Result<()>;
    
    /// GetRouteTools returns the parsed route tools
    fn get_route_tools(&self) -> &[RouteTool];
}