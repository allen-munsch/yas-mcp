use crate::internal::parser::adjuster::Adjuster;
use crate::internal::parser::types::{Parser, RouteTool};
use crate::internal::requester::types::RouteConfig;
use anyhow::{Context, Result};
use openapiv3::{OpenAPI, Parameter, ReferenceOr, Schema, SchemaKind, Type};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

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
                if let Some(Value::String(t)) = map.get("type") {
                    if t == "object" && !map.contains_key("properties") {
                        map.insert("properties".to_string(), serde_json::json!({}));
                    }
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

    fn schema_to_json_schema(&self, schema_ref: &ReferenceOr<Schema>) -> serde_json::Value {
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
                    properties.insert(name.clone(), self.schema_to_json_schema(&inner_schema));
                }

                let mut json = serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "description": description
                });

                if !obj.required.is_empty() {
                    if let Some(map) = json.as_object_mut() {
                        map.insert("required".to_string(), serde_json::json!(obj.required));
                    }
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
                        self.schema_to_json_schema(&inner_schema)
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

        if let Some(content) = request_body.content.get("application/json") {
            if let Some(schema) = &content.schema {
                let mut json_schema = self.schema_to_json_schema(schema);
                Self::ensure_strict_object(&mut json_schema);
                return Some(json_schema);
            }
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

        if matches!(route.method.as_str(), "POST" | "PUT" | "PATCH") {
            if let Some(body_schema) = self.get_body_schema(route) {
                properties.insert("body".to_string(), body_schema);
                required.push("body".to_string());
            }
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

        rmcp::model::Tool {
            name: tool_name.into(),
            title: None,
            description: Some(description.into()),
            input_schema: final_input.into(),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }
}

impl Parser for SwaggerParser {
    fn init(&mut self, swagger_path: &str, _adjustments_path: Option<&str>) -> Result<()> {
        let data = std::fs::read(swagger_path).context("Failed to read Swagger file")?;

        let mut json_value: serde_json::Value = if let Ok(v) = serde_json::from_slice(&data) {
            v
        } else if let Ok(v) = serde_yaml::from_slice::<serde_json::Value>(&data) {
            v
        } else {
            return Err(anyhow::anyhow!("Failed to parse spec as JSON or YAML"));
        };

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

                        let route_config = RouteConfig {
                            path: path.clone(),
                            method: method.to_string(),
                            description: op
                                .summary
                                .clone()
                                .or(op.description.clone())
                                .unwrap_or_default(),
                            method_config: crate::internal::requester::types::MethodConfig {
                                query_params: query_params,
                                header_params: header_params,
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
