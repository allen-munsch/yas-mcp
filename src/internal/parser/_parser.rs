// src/internal/parser/parser.rs

use anyhow::{anyhow, Context, Result};
use openapiv3::{OpenAPI, Operation, Parameter, ReferenceOr, Schema, SchemaKind, Type};
use serde_json::Map;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::sync::Arc;
use tracing::{info, warn};

use super::adjuster::Adjuster;
use super::types::{Parser, RouteTool};
use crate::internal::requester::{MethodConfig, RouteConfig};

/// SwaggerParser parses Swagger specifications and generates route configurations
pub struct SwaggerParser {
    doc: Option<OpenAPI>,
    route_tools: Vec<RouteTool>,
    adjuster: Adjuster,
}

impl SwaggerParser {
    pub fn new(adjuster: Adjuster) -> Self {
        Self {
            doc: None,
            route_tools: Vec::new(),
            adjuster,
        }
    }

    /// Detect and parse OpenAPI specification (focus on 3.0 first)
    fn detect_and_parse_openapi(&mut self, data: &[u8]) -> Result<()> {
        // Try JSON first
        if let Ok(doc) = serde_json::from_slice::<OpenAPI>(data) {
            info!("Successfully parsed OpenAPI 3.0 spec as JSON");
            self.doc = Some(doc);
            return Ok(());
        }

        // Try YAML
        if let Ok(doc) = serde_yaml::from_slice::<OpenAPI>(data) {
            info!("Successfully parsed OpenAPI 3.0 spec as YAML");
            self.doc = Some(doc);
            return Ok(());
        }

        // For now, skip OpenAPI 2.0 conversion to keep it simple
        warn!("OpenAPI 2.0 support temporarily disabled. Please use OpenAPI 3.0 specifications.");
        Err(anyhow!(
            "Failed to parse OpenAPI 3.0 spec from provided data"
        ))
    }

    fn generate_tool(&self, route: &RouteConfig) -> rmcp::model::Tool {
        // Create a normalized tool name
        let tool_name = Self::normalize_tool_name(&route.path, &route.method);

        let description = format!("{} {} - {}", route.method, route.path, route.description);

        // Create input schema based on route parameters
        let input_schema = self.create_input_schema(route);

        // Create output schema from responses
        let output_schema = self.create_output_schema(route);

        rmcp::model::Tool {
            name: tool_name.into(),
            description: Some(description.into()),
            input_schema: Arc::new(input_schema),
            annotations: None,
            icons: None,
            meta: None,
            title: None,
            output_schema: output_schema.map(Arc::new),
        }
    }

    /// Normalize tool names to avoid ambiguity
    fn normalize_tool_name(path: &str, method: &str) -> String {
        let path = path.trim_start_matches('/');
        let path = path.replace('/', "_");
        // Keep parameter names but make them distinct
        let path = path.replace(['{', '}'], "__");
        format!("{}_{}", method.to_lowercase(), path).to_lowercase()
    }

    /// Create JSON schema for tool inputs
    fn create_input_schema(&self, route: &RouteConfig) -> Map<String, serde_json::Value> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        // Add path parameters
        let path_params = Self::extract_path_params(&route.path);
        for param in &path_params {
            properties.insert(
                param.clone(),
                serde_json::json!({
                    "type": "string",
                    "description": format!("Path parameter: {}", param)
                }),
            );
        }
        required.extend(path_params);

        // Add query parameters with proper types
        for param in &route.method_config.query_params {
            if let Some(param_schema) = self.get_parameter_schema(route, param, "query") {
                properties.insert(param.clone(), param_schema);
            } else {
                // Fallback to string
                properties.insert(
                    param.clone(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("Query parameter: {}", param)
                    }),
                );
            }
        }

        // For POST/PUT/PATCH, add body parameter
        if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
            if let Some(body_schema) = self.get_body_schema(route) {
                properties.insert("body".to_string(), body_schema);
            }
        }

        let mut schema = Map::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );

        if !properties.is_empty() {
            schema.insert(
                "properties".to_string(),
                serde_json::Value::Object(properties),
            );
        }

        if !required.is_empty() {
            schema.insert(
                "required".to_string(),
                serde_json::Value::Array(
                    required
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }

        schema
    }

    /// Get parameter schema with proper type information
    fn get_parameter_schema(
        &self,
        route: &RouteConfig,
        param_name: &str,
        param_type: &str,
    ) -> Option<serde_json::Value> {
        let doc = self.doc.as_ref()?;
        let path_item = doc.paths.paths.get(&route.path)?;

        let path_item = match path_item {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => return None,
        };

        let operation = match route.method.as_str() {
            "GET" => path_item.get.as_ref(),
            "POST" => path_item.post.as_ref(),
            "PUT" => path_item.put.as_ref(),
            "DELETE" => path_item.delete.as_ref(),
            "PATCH" => path_item.patch.as_ref(),
            _ => None,
        }?;

        // Find the parameter in the operation
        for param in &operation.parameters {
            if let ReferenceOr::Item(param) = param {
                let param_data = match param {
                    Parameter::Query { parameter_data, .. } if param_type == "query" => {
                        parameter_data
                    }
                    Parameter::Path { parameter_data, .. } if param_type == "path" => {
                        parameter_data
                    }
                    Parameter::Header { parameter_data, .. } if param_type == "header" => {
                        parameter_data
                    }
                    _ => continue,
                };

                if param_data.name == param_name {
                    // Extract schema information from parameter
                    return Some(self.parameter_data_to_json_schema(param_data));
                }
            }
        }

        None
    }

    /// Convert ParameterData to JSON Schema
    fn parameter_data_to_json_schema(
        &self,
        param_data: &openapiv3::ParameterData,
    ) -> serde_json::Value {
        // For now, use string as default - in future, extract from param_data.format
        serde_json::json!({
            "type": "string",
            "description": param_data.description.as_deref().unwrap_or(""),
            "required": param_data.required,
        })
    }

    /// Create output schema from response definitions
    fn create_output_schema(&self, route: &RouteConfig) -> Option<Map<String, serde_json::Value>> {
        let doc = self.doc.as_ref()?;
        let path_item = doc.paths.paths.get(&route.path)?;
        let path_item = match path_item {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => return None,
        };

        let operation = match route.method.as_str() {
            "GET" => path_item.get.as_ref(),
            "POST" => path_item.post.as_ref(),
            "PUT" => path_item.put.as_ref(),
            "DELETE" => path_item.delete.as_ref(),
            "PATCH" => path_item.patch.as_ref(),
            _ => None,
        }?;

        // Get the first successful response (2xx)
        for (status, response_ref) in &operation.responses.responses {
            let status_code = match status {
                openapiv3::StatusCode::Code(code) => *code,
                _ => continue,
            };

            if (200..300).contains(&status_code) {
                if let ReferenceOr::Item(response) = response_ref {
                    if let Some(schema) = Self::get_first_response_schema(response) {
                        let json_schema = Self::schema_to_json_schema(&schema);
                        if let serde_json::Value::Object(mut map) = json_schema {
                            // Add status code info
                            map.insert(
                                "http_status".to_string(),
                                serde_json::Value::Number(status_code.into()),
                            );
                            return Some(map);
                        }
                    }
                }
            }
        }

        None
    }

    /// Get the first schema from a response
    fn get_first_response_schema(response: &openapiv3::Response) -> Option<Schema> {
        for (_, media_type) in &response.content {
            if let Some(ReferenceOr::Item(schema)) = &media_type.schema {
                return Some(schema.clone());
            }
        }
        None
    }

    /// Get the first body schema from an operation
    fn get_first_body_schema(operation: &Operation) -> (Option<Schema>, bool) {
        let request_body = match operation.request_body.as_ref() {
            Some(rb) => rb,
            None => return (None, false),
        };

        let request_body = match request_body {
            ReferenceOr::Item(rb) => rb,
            ReferenceOr::Reference { .. } => return (None, false),
        };

        let content = &request_body.content;
        if content.is_empty() {
            return (None, request_body.required);
        }

        // Try to find the first schema in any content type
        for (_, media_type) in content {
            if let Some(ReferenceOr::Item(schema)) = &media_type.schema {
                return (Some(schema.clone()), request_body.required);
            }
        }

        (None, request_body.required)
    }

    /// Get body schema for a route
    fn get_body_schema(&self, route: &RouteConfig) -> Option<serde_json::Value> {
        let doc = self.doc.as_ref()?;

        // Get the path item
        let path_item = doc.paths.paths.get(&route.path)?;
        let path_item = match path_item {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => return None,
        };

        let operation = match route.method.as_str() {
            "POST" => path_item.post.as_ref(),
            "PUT" => path_item.put.as_ref(),
            "PATCH" => path_item.patch.as_ref(),
            _ => None,
        }?;

        let (schema, _required) = Self::get_first_body_schema(operation);
        schema.map(|s| Self::schema_to_json_schema(&s))
    }

    /// Convert OpenAPI schema to JSON schema
    fn schema_to_json_schema(schema: &Schema) -> serde_json::Value {
        match &schema.schema_kind {
            SchemaKind::Type(Type::Object(object_type)) => {
                let mut properties = serde_json::Map::new();
                for (prop_name, prop_schema) in &object_type.properties {
                    if let ReferenceOr::Item(prop_schema) = prop_schema {
                        properties
                            .insert(prop_name.clone(), Self::schema_to_json_schema(prop_schema));
                    }
                }
                let mut result = serde_json::Map::new();
                result.insert(
                    "type".to_string(),
                    serde_json::Value::String("object".to_string()),
                );
                result.insert(
                    "properties".to_string(),
                    serde_json::Value::Object(properties),
                );
                if !object_type.required.is_empty() {
                    result.insert(
                        "required".to_string(),
                        serde_json::Value::Array(
                            object_type
                                .required
                                .iter()
                                .map(|s| serde_json::Value::String(s.clone()))
                                .collect(),
                        ),
                    );
                }
                serde_json::Value::Object(result)
            }
            SchemaKind::Type(Type::String(_)) => {
                serde_json::json!({
                    "type": "string",
                    "description": schema.schema_data.description,
                })
            }
            SchemaKind::Type(Type::Number(_)) => {
                serde_json::json!({
                    "type": "number",
                    "description": schema.schema_data.description,
                })
            }
            SchemaKind::Type(Type::Integer(_)) => {
                serde_json::json!({
                    "type": "integer",
                    "description": schema.schema_data.description,
                })
            }
            SchemaKind::Type(Type::Boolean(_)) => {
                serde_json::json!({
                    "type": "boolean",
                    "description": schema.schema_data.description,
                })
            }
            SchemaKind::Type(Type::Array(array_type)) => {
                // Fixed: items_box is already ReferenceOr<Box<Schema>>, not Box<ReferenceOr<Schema>>
                let items = array_type
                    .items
                    .as_ref()
                    .map(|items_ref_or| {
                        // Match directly on the ReferenceOr without calling as_ref()
                        match items_ref_or {
                            ReferenceOr::Item(item_schema) => {
                                Self::schema_to_json_schema(item_schema)
                            }
                            ReferenceOr::Reference { .. } => serde_json::json!({}),
                        }
                    })
                    .unwrap_or(serde_json::json!({}));

                serde_json::json!({
                    "type": "array",
                    "items": items,
                    "description": schema.schema_data.description,
                })
            }
            _ => {
                serde_json::json!({
                    "type": "object",
                    "description": schema.schema_data.description,
                })
            }
        }
    }

    /// Extract path parameters from a URL path
    fn extract_path_params(path: &str) -> Vec<String> {
        path.split('/')
            .filter(|part| part.starts_with('{') && part.ends_with('}'))
            .map(|part| {
                part.trim_start_matches('{')
                    .trim_end_matches('}')
                    .to_string()
            })
            .collect()
    }

    /// Process operations from the parsed OpenAPI document
    fn process_operations(&mut self) -> Result<()> {
        let doc = self
            .doc
            .as_ref()
            .ok_or_else(|| anyhow!("No OpenAPI document loaded"))?;

        info!("Processing operations from OpenAPI document");

        // Debug: Check adjuster state
        info!(
            "Adjuster routes count: {}",
            self.adjuster.get_routes_count()
        );
        if self.adjuster.get_routes_count() > 0 {
            info!("Adjuster is configured to filter routes");
        } else {
            info!("Adjuster has no route filters - all routes should be allowed");
        }

        info!("Total paths in document: {}", doc.paths.paths.len());

        if doc.paths.paths.is_empty() {
            warn!("No paths found in OpenAPI document!");
            return Ok(());
        }

        for (path, path_item) in doc.paths.iter() {
            info!("Found path: {}", path);

            let path_item = match path_item {
                ReferenceOr::Item(item) => {
                    info!("  Path item is direct reference");
                    item
                }
                ReferenceOr::Reference { reference } => {
                    info!("  Path item is reference: {}", reference);
                    continue;
                }
            };

            let methods = vec![
                ("GET", path_item.get.as_ref()),
                ("POST", path_item.post.as_ref()),
                ("PUT", path_item.put.as_ref()),
                ("DELETE", path_item.delete.as_ref()),
                ("PATCH", path_item.patch.as_ref()),
            ];

            for (method, operation) in methods {
                if let Some(operation) = operation {
                    info!("  Found operation: {} {}", method, path);

                    let route_config = self.create_route_config(path, method, operation);

                    // Call adjuster and log the result
                    let exists = self
                        .adjuster
                        .exists_in_mcp(&route_config.path, &route_config.method);
                    info!("  Adjuster result for {} {}: {}", method, path, exists);

                    if exists {
                        let tool = self.generate_tool(&route_config);
                        self.route_tools.push(RouteTool { route_config, tool });
                        info!("  ✅ Added tool for {} {}", method, path);
                    } else {
                        info!(
                            "  ❌ Skipped tool for {} {} (filtered by adjuster)",
                            method, path
                        );
                    }
                }
            }
        }

        info!("Processed {} route tools", self.route_tools.len());
        Ok(())
    }

    /// Create a route configuration from a path and operation
    fn create_route_config(&self, path: &str, method: &str, operation: &Operation) -> RouteConfig {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let mut description = String::new();
        if let Some(desc) = &operation.description {
            description = desc.clone();
        } else if let Some(summary) = &operation.summary {
            description = summary.clone();
        }

        // Apply adjustments if any
        let description = self.adjuster.get_description(path, method, &description);

        // Add operation-specific headers
        let responses = &operation.responses;
        for response in responses.responses.values() {
            if let ReferenceOr::Item(response) = response {
                if let Some(content_type) = response.content.keys().next() {
                    headers.insert("Accept".to_string(), content_type.clone());
                    break;
                }
            }
        }

        // Extract query parameters
        let mut query_params = Vec::new();
        for param in &operation.parameters {
            if let ReferenceOr::Item(Parameter::Query { parameter_data, .. }) = param {
                query_params.push(parameter_data.name.clone());
            }
        }

        // Extract path parameters for the parameters map
        let mut parameters = HashMap::new();
        let path_params = Self::extract_path_params(path);
        for param in path_params {
            parameters.insert(param.clone(), "".to_string());
        }

        RouteConfig {
            path: path.to_string(),
            method: method.to_string(),
            description,
            headers,
            parameters,
            method_config: MethodConfig {
                query_params,
                form_fields: Vec::new(),
                file_upload: None,
            },
        }
    }
}

impl Parser for SwaggerParser {
    fn init(&mut self, openapi_spec: &str, adjustments_file: Option<&str>) -> Result<()> {
        let data = fs::read(openapi_spec)
            .with_context(|| format!("Failed to read spec file: {}", openapi_spec))?;

        if let Some(adjustments_file) = adjustments_file {
            self.adjuster.load(adjustments_file)?;
        }

        self.detect_and_parse_openapi(&data)?;
        self.process_operations()?;

        Ok(())
    }

    fn parse_reader(&mut self, mut reader: Box<dyn Read>) -> Result<()> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;

        self.detect_and_parse_openapi(&data)?;
        self.process_operations()?;

        Ok(())
    }

    fn get_route_tools(&self) -> &[RouteTool] {
        &self.route_tools
    }
}
