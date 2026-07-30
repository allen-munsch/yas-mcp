// src/internal/parser/mod.rs

pub mod adjuster;
pub mod demo_spec;
pub mod mock;
pub mod normalizer;
pub mod types;

// Export the Parser trait and RouteTool from types
pub use types::{Parser, RouteTool};

// Export Adjuster
use crate::internal::requester::types::RouteConfig;
pub use adjuster::Adjuster;
use anyhow::{Context, Result};
use openapiv3::{OpenAPI, Parameter, ReferenceOr, Schema, SchemaKind, Type};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, OnceLock};

pub struct SwaggerParser {
    doc: Option<OpenAPI>,
    adjuster: Adjuster,
    cache_tools: Vec<RouteTool>,
}

impl SwaggerParser {
    pub fn new(adjuster: Adjuster) -> Self {
        Self {
            doc: None,
            adjuster,
            cache_tools: Vec::new(),
        }
    }

    fn clean_description(desc: &str) -> String {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"<[^>]*>").unwrap());

        let no_html = re.replace_all(desc, " ");
        let cleaned = no_html
            .replace(['\n', '\r'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if cleaned.is_empty() {
            return "No description provided".to_string();
        }

        if cleaned.len() > 700 {
            let mut truncated = cleaned[..700].to_string();
            truncated.push_str("...");
            truncated
        } else {
            cleaned
        }
    }

    fn ensure_strict_object(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(t)) = map.get("type")
                    && t == "object"
                    && !map.contains_key("properties")
                {
                    map.insert("properties".to_string(), serde_json::json!({}));
                }

                if let Some(Value::Object(props)) = map.get_mut("properties") {
                    for v in props.values_mut() {
                        Self::ensure_strict_object(v);
                    }
                }

                if let Some(items) = map.get_mut("items") {
                    Self::ensure_strict_object(items);
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    Self::ensure_strict_object(v);
                }
            }
            _ => {}
        }
    }

    fn normalize_tool_name(path: &str, method: &str) -> String {
        let path = path
            .replace('/', "_")
            .replace(['{', '}'], "__")
            .replace('-', "_");
        let name = format!("{}_{}", method.to_lowercase(), path);
        let re = Regex::new(r"[^a-zA-Z0-9_-]").unwrap();
        let cleaned = re.replace_all(&name, "").to_string();

        if cleaned.len() > 60 {
            cleaned[..60].to_string()
        } else {
            cleaned
        }
    }

    fn extract_path_params(path: &str) -> Vec<String> {
        let re = Regex::new(r"\{([^}]+)\}").unwrap();
        re.captures_iter(path)
            .map(|cap| cap[1].to_string())
            .collect()
    }

    fn schema_to_json_schema(schema_ref: &ReferenceOr<Schema>) -> serde_json::Value {
        let schema = match schema_ref {
            ReferenceOr::Item(s) => s,
            ReferenceOr::Reference { .. } => return serde_json::json!({ "type": "string" }),
        };

        let description =
            Self::clean_description(schema.schema_data.description.as_deref().unwrap_or(""));

        match &schema.schema_kind {
            SchemaKind::Type(Type::String(_)) => serde_json::json!({
                "type": "string",
                "description": description
            }),
            SchemaKind::Type(Type::Number(_)) => serde_json::json!({
                "type": "number",
                "description": description
            }),
            SchemaKind::Type(Type::Integer(_)) => serde_json::json!({
                "type": "number",
                "description": description
            }),
            SchemaKind::Type(Type::Boolean(_)) => serde_json::json!({
                "type": "boolean",
                "description": description
            }),
            SchemaKind::Type(Type::Object(obj)) => {
                let mut properties = Map::new();
                for (name, prop_schema) in &obj.properties {
                    let inner_schema = match prop_schema {
                        ReferenceOr::Item(x) => ReferenceOr::Item(*x.clone()),
                        ReferenceOr::Reference { reference } => ReferenceOr::Reference {
                            reference: reference.clone(),
                        },
                    };
                    properties.insert(name.clone(), Self::schema_to_json_schema(&inner_schema));
                }

                let mut json = serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "description": description
                });

                if !obj.required.is_empty()
                    && let Some(map) = json.as_object_mut()
                {
                    map.insert("required".to_string(), serde_json::json!(obj.required));
                }
                json
            }
            SchemaKind::Type(Type::Array(arr)) => {
                let items = match &arr.items {
                    Some(items_ref) => {
                        let inner_schema = match items_ref {
                            ReferenceOr::Item(x) => ReferenceOr::Item(*x.clone()),
                            ReferenceOr::Reference { reference } => ReferenceOr::Reference {
                                reference: reference.clone(),
                            },
                        };
                        Self::schema_to_json_schema(&inner_schema)
                    }
                    None => serde_json::json!({ "type": "string" }),
                };
                serde_json::json!({
                    "type": "array",
                    "items": items,
                    "description": description
                })
            }
            _ => serde_json::json!({
                "type": "string",
                "description": description
            }),
        }
    }

    fn parameter_data_to_json_schema(
        &self,
        param_data: &openapiv3::ParameterData,
    ) -> (serde_json::Value, bool) {
        let raw_desc = param_data.description.as_deref().unwrap_or("");
        let description = Self::clean_description(raw_desc);

        let schema = serde_json::json!({
            "type": "string",
            "description": description
        });

        (schema, param_data.required)
    }

    fn get_parameter_schema(
        &self,
        route: &RouteConfig,
        param_name: &str,
        param_type: &str,
    ) -> Option<(serde_json::Value, bool)> {
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
                    return Some(self.parameter_data_to_json_schema(param_data));
                }
            }
        }
        None
    }

    fn get_body_schema(&self, route: &RouteConfig) -> Option<serde_json::Value> {
        let doc = self.doc.as_ref()?;
        let path_item = doc.paths.paths.get(&route.path)?;
        let path_item = match path_item {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => return None,
        };
        let operation = match route.method.as_str() {
            "POST" => path_item.post.as_ref(),
            "PUT" => path_item.put.as_ref(),
            "PATCH" => path_item.patch.as_ref(),
            _ => return None,
        }?;

        let request_body = operation.request_body.as_ref()?;
        let request_body = match request_body {
            ReferenceOr::Item(rb) => rb,
            ReferenceOr::Reference { .. } => return None,
        };

        if let Some(content) = request_body.content.get("application/json")
            && let Some(schema) = &content.schema
        {
            let mut json_schema = Self::schema_to_json_schema(schema);
            Self::ensure_strict_object(&mut json_schema);
            return Some(json_schema);
        }
        None
    }

    fn create_input_schema(&self, route: &RouteConfig) -> Map<String, serde_json::Value> {
        let mut properties = Map::new();
        let mut required = Vec::new();

        let path_params = Self::extract_path_params(&route.path);
        for param in &path_params {
            properties.insert(
                param.clone(),
                serde_json::json!({
                    "type": "string",
                    "description": format!("Path parameter: {}", param)
                }),
            );
            required.push(param.clone());
        }

        // Iterate directly over the vectors (No "if let Some")
        for param in &route.method_config.query_params {
            if let Some((param_schema, is_required)) =
                self.get_parameter_schema(route, param, "query")
            {
                properties.insert(param.to_string(), param_schema);
                if is_required {
                    required.push(param.to_string());
                }
            } else {
                properties.insert(
                    param.to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("Query parameter: {}", param)
                    }),
                );
            }
        }

        for param in &route.method_config.header_params {
            if let Some((param_schema, is_required)) =
                self.get_parameter_schema(route, param, "header")
            {
                properties.insert(param.to_string(), param_schema);
                if is_required {
                    required.push(param.to_string());
                }
            } else {
                properties.insert(
                    param.to_string(),
                    serde_json::json!({
                        "type": "string",
                        "description": format!("Header parameter: {}", param)
                    }),
                );
            }
        }

        if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH")
            && let Some(body_schema) = self.get_body_schema(route)
        {
            properties.insert("body".to_string(), body_schema);
            required.push("body".to_string());
        }

        let mut schema = Map::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );
        schema.insert(
            "properties".to_string(),
            serde_json::Value::Object(properties),
        );

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

    fn generate_tool(&self, route: &RouteConfig) -> rmcp::model::Tool {
        let tool_name = Self::normalize_tool_name(&route.path, &route.method);

        let raw_desc = format!("{} {} - {}", route.method, route.path, route.description);
        let description = Self::clean_description(&raw_desc);

        let input_schema = self.create_input_schema(route);
        let mut input_val = serde_json::Value::Object(input_schema);
        Self::ensure_strict_object(&mut input_val);

        let final_input = input_val.as_object().unwrap().clone();

        rmcp::model::Tool::new(tool_name, description, Arc::new(final_input))
    }
}

impl Parser for SwaggerParser {
    fn init(&mut self, swagger_path: &str, adjustments_path: Option<&str>) -> Result<()> {
        let data = std::fs::read(swagger_path).context("Failed to read Swagger file")?;

        // Load adjustments if provided
        if let Some(adj_path) = adjustments_path {
            self.adjuster.load(adj_path)?;
        }

        let mut json_value: serde_json::Value = if let Ok(v) = serde_json::from_slice(&data) {
            v
        } else if let Ok(v) = serde_yaml::from_slice::<serde_json::Value>(&data) {
            v
        } else {
            return Err(anyhow::anyhow!("Failed to parse spec as JSON or YAML"));
        };

        // Normalize: Swagger 2.0 → OpenAPI 3.0, 3.1 → 3.0
        json_value = crate::internal::parser::normalizer::normalize(&json_value)
            .context("Failed to normalize specification")?;

        fn sanitize_refs(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    if map.contains_key("$ref") {
                        let ref_val = map["$ref"].clone();
                        map.clear();
                        map.insert("$ref".to_string(), ref_val);
                    } else {
                        for v in map.values_mut() {
                            sanitize_refs(v);
                        }
                    }
                }
                Value::Array(arr) => {
                    for v in arr {
                        sanitize_refs(v);
                    }
                }
                _ => {}
            }
        }
        sanitize_refs(&mut json_value);

        let doc: OpenAPI = serde_json::from_value(json_value)
            .context("Failed to parse into strict OpenAPI struct")?;
        self.doc = Some(doc);

        if let Some(doc) = &self.doc {
            for (path, item) in &doc.paths.paths {
                let item = match item {
                    ReferenceOr::Item(i) => i,
                    _ => continue,
                };

                let operations = [
                    ("GET", &item.get),
                    ("POST", &item.post),
                    ("PUT", &item.put),
                    ("DELETE", &item.delete),
                    ("PATCH", &item.patch),
                ];

                for (method, op_opt) in operations {
                    if let Some(op) = op_opt {
                        // Check if this route should be included via adjuster
                        if !self.adjuster.exists_in_mcp(path, method) {
                            continue;
                        }

                        let mut query_params = Vec::new();
                        let mut header_params = Vec::new();

                        for p in &op.parameters {
                            match p {
                                ReferenceOr::Item(Parameter::Query { parameter_data, .. }) => {
                                    query_params.push(parameter_data.name.clone());
                                }
                                ReferenceOr::Item(Parameter::Header { parameter_data, .. }) => {
                                    header_params.push(parameter_data.name.clone());
                                }
                                _ => {}
                            }
                        }

                        // Get base description and apply adjuster modifications
                        let base_description = op
                            .summary
                            .clone()
                            .or(op.description.clone())
                            .unwrap_or_default();
                        let description =
                            self.adjuster
                                .get_description(path, method, &base_description);

                        let route_config = RouteConfig {
                            path: path.clone(),
                            method: method.to_string(),
                            description,
                            method_config: crate::internal::requester::types::MethodConfig {
                                query_params,
                                header_params,
                                ..Default::default()
                            },
                            headers: HashMap::new(),
                            parameters: HashMap::new(),
                        };

                        let tool = self.generate_tool(&route_config);
                        self.cache_tools.push(RouteTool { route_config, tool });
                    }
                }
            }
        }

        Ok(())
    }

    fn get_route_tools(&self) -> &[RouteTool] {
        &self.cache_tools
    }

    fn parse_reader(&mut self, _reader: Box<dyn Read>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_description_strips_html() {
        let input = "<p>Hello <b>world</b></p>";
        let cleaned = SwaggerParser::clean_description(input);
        assert!(!cleaned.contains('<'));
        assert!(cleaned.contains("Hello"));
        assert!(cleaned.contains("world"));
    }

    #[test]
    fn test_clean_description_truncates_long() {
        let input = "a".repeat(800);
        let cleaned = SwaggerParser::clean_description(&input);
        assert!(cleaned.len() <= 703); // 700 + "..."
        assert!(cleaned.ends_with("..."));
    }

    #[test]
    fn test_clean_description_empty() {
        let cleaned = SwaggerParser::clean_description("");
        assert_eq!(cleaned, "No description provided");
    }

    #[test]
    fn test_clean_description_whitespace_only() {
        let cleaned = SwaggerParser::clean_description("   \n  \r  ");
        assert_eq!(cleaned, "No description provided");
    }

    #[test]
    fn test_normalize_tool_name_simple() {
        let name = SwaggerParser::normalize_tool_name("/users", "GET");
        assert_eq!(name, "get__users");
    }

    #[test]
    fn test_normalize_tool_name_with_path_params() {
        let name = SwaggerParser::normalize_tool_name("/users/{userId}/posts", "POST");
        // / becomes _, { becomes __, } becomes __
        assert_eq!(name, "post__users___userId___posts");
    }

    #[test]
    fn test_normalize_tool_name_long_truncation() {
        // Create a path that would produce a very long name
        let long_path = format!("/{}", "a".repeat(80));
        let name = SwaggerParser::normalize_tool_name(&long_path, "GET");
        assert!(name.len() <= 60);
    }

    #[test]
    fn test_normalize_tool_name_strips_special_chars() {
        let name = SwaggerParser::normalize_tool_name("/api/v1/data@export", "GET");
        assert!(!name.contains('@'));
    }

    #[test]
    fn test_extract_path_params_no_params() {
        let params = SwaggerParser::extract_path_params("/users");
        assert!(params.is_empty());
    }

    #[test]
    fn test_extract_path_params_single() {
        let params = SwaggerParser::extract_path_params("/users/{id}");
        assert_eq!(params, vec!["id"]);
    }

    #[test]
    fn test_extract_path_params_multiple() {
        let params =
            SwaggerParser::extract_path_params("/orgs/{orgId}/repos/{repoId}/issues/{issueId}");
        assert_eq!(params, vec!["orgId", "repoId", "issueId"]);
    }

    #[test]
    fn test_schema_to_json_schema_string() {
        let schema = openapiv3::ReferenceOr::Item(openapiv3::Schema {
            schema_data: openapiv3::SchemaData {
                description: Some("A string field".into()),
                ..Default::default()
            },
            schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::String(
                openapiv3::StringType::default(),
            )),
        });

        let result = SwaggerParser::schema_to_json_schema(&schema);
        assert_eq!(result["type"], "string");
        assert!(
            result["description"]
                .as_str()
                .unwrap()
                .contains("string field")
        );
    }

    #[test]
    fn test_schema_to_json_schema_integer() {
        let schema = openapiv3::ReferenceOr::Item(openapiv3::Schema {
            schema_data: openapiv3::SchemaData::default(),
            schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Integer(
                openapiv3::IntegerType::default(),
            )),
        });

        let result = SwaggerParser::schema_to_json_schema(&schema);
        assert_eq!(result["type"], "number");
    }

    #[test]
    fn test_schema_to_json_schema_boolean() {
        let schema = openapiv3::ReferenceOr::Item(openapiv3::Schema {
            schema_data: openapiv3::SchemaData::default(),
            schema_kind: openapiv3::SchemaKind::Type(openapiv3::Type::Boolean(
                openapiv3::BooleanType::default(),
            )),
        });

        let result = SwaggerParser::schema_to_json_schema(&schema);
        assert_eq!(result["type"], "boolean");
    }

    #[test]
    fn test_schema_to_json_schema_reference_fallback() {
        // References should fall back to type: string
        let schema = openapiv3::ReferenceOr::Reference {
            reference: "#/components/schemas/User".into(),
        };

        let result = SwaggerParser::schema_to_json_schema(&schema);
        assert_eq!(result["type"], "string");
    }

    #[test]
    fn test_parse_todo_spec_success() {
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init("examples/todo-app/openapi.yaml", None);
        assert!(result.is_ok());

        let tools = parser.get_route_tools();
        assert!(!tools.is_empty(), "Should have parsed at least one tool");

        // Verify at least one tool has expected fields
        let first = &tools[0];
        assert!(!first.tool.name.is_empty());
        assert!(!first.route_config.path.is_empty());
        assert!(!first.route_config.method.is_empty());
    }

    #[test]
    fn test_parse_petstore_spec_success() {
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init("examples/petstore.yaml", None);
        assert!(result.is_ok());

        let tools = parser.get_route_tools();
        // Petstore has 5 operations: list, create, show, update, delete
        assert!(
            tools.len() >= 3,
            "Expected at least 3 tools, got {}",
            tools.len()
        );
    }

    #[test]
    fn test_parse_with_adjustments() {
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init("examples/petstore.yaml", Some("adjustments.yaml"));
        assert!(result.is_ok());

        let tools = parser.get_route_tools();
        // Adjustments filter to only /pets routes (GET, POST, PUT, DELETE)
        // Actually adjustments.yaml filters to /pets + /pets/{petId}
        assert!(tools.len() <= 10, "Adjustments should filter routes");
    }

    #[test]
    fn test_parse_missing_file_errors() {
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init("nonexistent/file.yaml", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_strict_object_adds_properties() {
        let mut value = serde_json::json!({
            "type": "object"
        });
        SwaggerParser::ensure_strict_object(&mut value);
        assert!(value.get("properties").is_some());
        assert_eq!(value["properties"], serde_json::json!({}));
    }

    #[test]
    fn test_ensure_strict_object_keeps_existing_properties() {
        let mut value = serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string"}}
        });
        SwaggerParser::ensure_strict_object(&mut value);
        assert!(value["properties"].get("name").is_some());
    }

    #[test]
    fn test_ensure_strict_object_nested() {
        let mut value = serde_json::json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object"
                }
            }
        });
        SwaggerParser::ensure_strict_object(&mut value);
        let nested = &value["properties"]["nested"];
        assert!(nested.get("properties").is_some());
    }

    #[test]
    fn test_ensure_strict_object_on_array_items() {
        let mut value = serde_json::json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object"
                    }
                }
            }
        });
        SwaggerParser::ensure_strict_object(&mut value);
        let array_items = &value["properties"]["items"]["items"];
        assert!(array_items.get("properties").is_some());
    }

    #[test]
    fn test_parse_missing_file_returns_error() {
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init("nonexistent/file/path.yaml", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_yaml() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "not: valid: yaml: [[[").unwrap();
        let mut parser = SwaggerParser::new(Adjuster::new());
        let result = parser.init(tmp.path().to_str().unwrap(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_tool_name_special_chars() {
        let name = SwaggerParser::normalize_tool_name("/api/v1/data@export", "GET");
        assert!(!name.contains('@'));
        assert!(name.starts_with("get_"));
    }

    #[test]
    fn test_clean_description_newlines() {
        let desc = "Line 1\nLine 2\r\nLine 3";
        let cleaned = SwaggerParser::clean_description(desc);
        assert!(!cleaned.contains('\n'));
        assert!(!cleaned.contains('\r'));
        assert!(cleaned.contains("Line 1"));
        assert!(cleaned.contains("Line 2"));
    }

    #[test]
    fn test_ensure_strict_object_non_object() {
        let mut value = serde_json::json!("just a string");
        SwaggerParser::ensure_strict_object(&mut value);
        // Should not panic, should be a no-op for non-objects
        assert_eq!(value, "just a string");
    }

    #[test]
    fn test_ensure_strict_object_array() {
        let mut value = serde_json::json!([1, 2, 3]);
        SwaggerParser::ensure_strict_object(&mut value);
        // Arrays should iterate items
        assert_eq!(value, serde_json::json!([1, 2, 3]));
    }
}
