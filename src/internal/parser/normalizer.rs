//! OpenAPI / Swagger Normalizer
//!
//! Converts Swagger 2.0 and OpenAPI 3.1 specs into OpenAPI 3.0 format
//! before they reach the parser. The core parser only needs to understand
//! one format — this module handles the rest.
//!
//! # Supported Inputs
//!
//! | Version | Example | Strategy |
//! |---------|---------|----------|
//! | `swagger: "2.0"` | Swagger 2.0 | Full conversion to 3.0 |
//! | `openapi: "3.0.x"` | OpenAPI 3.0 | Pass through unchanged |
//! | `openapi: "3.1.x"` | OpenAPI 3.1 | Downgrade to 3.0 (native 3.1 planned) |
//!
//! ## Future: Native OpenAPI 3.1
//!
//! OpenAPI 3.1 is fully JSON Schema compliant. Native support would preserve:
//! - `type: ["string", "null"]` → proper nullable in MCP tool schemas
//! - `$ref` with sibling properties → richer schema composition
//! - `examples` arrays → better tool documentation for AI assistants
//!
//! The plan: parse 3.1 specs through `jsonschema`/`schemars` directly,
//! bypassing the `openapiv3` crate entirely for 3.1 inputs. This gives
//! first-class JSON Schema fidelity in the generated MCP tools.

use anyhow::Result;
use serde_json::{Map, Value, json};
use tracing::{debug, info, warn};

/// Detected specification version
#[derive(Debug, Clone, PartialEq)]
pub enum SpecVersion {
    Swagger2,
    OpenApi30,
    OpenApi31,
    Unknown(String),
}

/// Detect the specification version from the raw JSON/YAML value
pub fn detect_version(value: &Value) -> SpecVersion {
    if let Some(swagger) = value.get("swagger").and_then(|v| v.as_str()) {
        if swagger.starts_with("2.") {
            return SpecVersion::Swagger2;
        }
    }

    if let Some(openapi) = value.get("openapi").and_then(|v| v.as_str()) {
        if openapi.starts_with("3.0") {
            return SpecVersion::OpenApi30;
        }
        if openapi.starts_with("3.1") || openapi.starts_with("3.") {
            return SpecVersion::OpenApi31;
        }
    }

    SpecVersion::Unknown(
        value
            .get("openapi")
            .or(value.get("swagger"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
    )
}

/// Normalize any supported spec version to OpenAPI 3.0 JSON.
///
/// Returns the normalized `serde_json::Value` ready for `openapiv3` parsing.
pub fn normalize(value: &Value) -> Result<Value> {
    let version = detect_version(value);

    match version {
        SpecVersion::Swagger2 => {
            info!("Detected Swagger 2.0 — converting to OpenAPI 3.0");
            convert_swagger2_to_openapi3(value)
        }
        SpecVersion::OpenApi30 => {
            debug!("Detected OpenAPI 3.0 — passing through");
            Ok(value.clone())
        }
        SpecVersion::OpenApi31 => {
            info!("Detected OpenAPI 3.1 — downgrading to 3.0");
            downgrade_openapi31_to_30(value)
        }
        SpecVersion::Unknown(v) => {
            // Try our best — pass through and let the parser reject if invalid
            warn!(
                "Unknown spec version '{}' — attempting to parse as OpenAPI 3.0",
                v
            );
            Ok(value.clone())
        }
    }
}

// ── Swagger 2.0 → OpenAPI 3.0 Conversion ──────────────────────────────────

fn convert_swagger2_to_openapi3(swagger: &Value) -> Result<Value> {
    let mut out = Map::new();

    // ── Top-level metadata ────────────────────────────────────────────
    out.insert("openapi".into(), json!("3.0.3"));

    if let Some(info) = swagger.get("info").cloned() {
        out.insert("info".into(), info);
    } else {
        out.insert(
            "info".into(),
            json!({"title": "Converted API", "version": "1.0.0"}),
        );
    }

    // ── Servers (from host + basePath + schemes) ──────────────────────
    let host = swagger
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("localhost");
    let base_path = swagger
        .get("basePath")
        .and_then(|v| v.as_str())
        .unwrap_or("/");
    let schemes = swagger.get("schemes").and_then(|v| v.as_array());

    let scheme = if let Some(schemes) = schemes {
        if schemes.iter().any(|s| s.as_str() == Some("https")) {
            "https"
        } else {
            schemes.first().and_then(|s| s.as_str()).unwrap_or("http")
        }
    } else {
        "http"
    };

    // If basePath doesn't start with /, add it
    let base_path = if base_path.starts_with('/') {
        base_path
    } else {
        &format!("/{base_path}")
    };

    let server_url = format!("{scheme}://{host}{base_path}");
    out.insert(
        "servers".into(),
        json!([{"url": server_url, "description": "Auto-generated from Swagger 2.0"}]),
    );

    // ── Paths (mostly compatible) ─────────────────────────────────────
    if let Some(paths) = swagger.get("paths") {
        let converted = convert_swagger2_paths(paths)?;
        out.insert("paths".into(), converted);
    }

    // ── Components (from definitions, parameters, responses) ──────────
    let mut components = Map::new();

    // definitions → components/schemas
    if let Some(defs) = swagger.get("definitions").and_then(|v| v.as_object()) {
        let schemas = convert_swagger2_definitions(defs)?;
        components.insert("schemas".into(), Value::Object(schemas));
    }

    // parameters → components/parameters
    if let Some(params) = swagger.get("parameters").and_then(|v| v.as_object()) {
        components.insert("parameters".into(), Value::Object(params.clone()));
    }

    // responses → components/responses
    if let Some(responses) = swagger.get("responses").and_then(|v| v.as_object()) {
        components.insert("responses".into(), Value::Object(responses.clone()));
    }

    if !components.is_empty() {
        out.insert("components".into(), Value::Object(components));
    }

    // ── Security ──────────────────────────────────────────────────────
    if let Some(security) = swagger.get("security").cloned() {
        out.insert("security".into(), security);
    }
    if let Some(security_defs) = swagger.get("securityDefinitions").cloned() {
        let mut components = out
            .get("components")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        if let Some(obj) = components.as_object_mut() {
            obj.insert("securitySchemes".into(), security_defs);
        }
        out.insert("components".into(), components);
    }

    // ── Tags ──────────────────────────────────────────────────────────
    if let Some(tags) = swagger.get("tags").cloned() {
        out.insert("tags".into(), tags);
    }

    // ── External docs ─────────────────────────────────────────────────
    if let Some(docs) = swagger.get("externalDocs").cloned() {
        out.insert("externalDocs".into(), docs);
    }

    debug!("Swagger 2.0 → OpenAPI 3.0 conversion complete");

    Ok(Value::Object(out))
}

/// Convert Swagger 2.0 paths to OpenAPI 3.0 paths.
/// The main differences are:
/// - Swagger 2.0 uses `operationId` directly (same as 3.0)
/// - Swagger 2.0 uses `parameters` with `in: body` for request body (3.0 uses `requestBody`)
/// - Swagger 2.0 uses `produces`/`consumes` at path level (3.0 uses per-operation or global)
fn convert_swagger2_paths(paths: &Value) -> Result<Value> {
    let path_obj = match paths.as_object() {
        Some(o) => o,
        None => return Ok(paths.clone()),
    };

    let mut out = Map::new();

    for (path, path_item) in path_obj {
        let item_obj = match path_item.as_object() {
            Some(o) => o,
            None => {
                out.insert(path.clone(), path_item.clone());
                continue;
            }
        };

        let mut new_item = Map::new();

        // Copy non-operation fields
        for (key, val) in item_obj {
            if !is_http_method(key) {
                new_item.insert(key.clone(), val.clone());
            }
        }

        // Convert each operation
        let methods = ["get", "post", "put", "delete", "patch", "options", "head"];
        let mut has_operations = false;

        for method in methods {
            if let Some(op) = item_obj.get(method) {
                let converted = convert_swagger2_operation(op, path)?;
                new_item.insert(method.to_string(), converted);
                has_operations = true;
            }
        }

        if has_operations || !new_item.is_empty() {
            out.insert(path.clone(), Value::Object(new_item));
        }
    }

    Ok(Value::Object(out))
}

/// Convert a single Swagger 2.0 operation to OpenAPI 3.0 format
fn convert_swagger2_operation(op: &Value, _path: &str) -> Result<Value> {
    let op_obj = match op.as_object() {
        Some(o) => o,
        None => return Ok(op.clone()),
    };

    let mut new_op = Map::new();

    for (key, val) in op_obj {
        match key.as_str() {
            // `consumes` → part of requestBody content types
            "consumes" => {
                // Handled below when building requestBody
                continue;
            }
            // `produces` → part of response content types
            "produces" => {
                // Handled below when building responses
                continue;
            }
            // Parameters need conversion: `in: body` → `requestBody`
            "parameters" => {
                if let Some(params) = val.as_array() {
                    let (query_params, body_param) = split_swagger2_params(params);
                    if !query_params.is_empty() {
                        new_op.insert("parameters".into(), Value::Array(query_params.clone()));
                    }
                    if let Some(body) = body_param {
                        let request_body = convert_body_param_to_request_body(&body);
                        new_op.insert("requestBody".into(), request_body);
                    }
                }
            }
            // Everything else passes through
            _ => {
                new_op.insert(key.clone(), val.clone());
            }
        }
    }

    // If operation has `produces`, wrap responses with content types
    if let Some(produces) = op_obj.get("produces").and_then(|v| v.as_array()) {
        if let Some(responses) = new_op.get_mut("responses") {
            if let Some(resp_obj) = responses.as_object_mut() {
                for (_code, resp) in resp_obj.iter_mut() {
                    if resp.is_object() && resp.get("content").is_none() {
                        let content = build_content_for_produces(produces, resp);
                        if let Some(resp_map) = resp.as_object_mut() {
                            resp_map.insert("content".into(), content);
                        }
                    }
                }
            }
        }
    }

    Ok(Value::Object(new_op))
}

/// Split parameters: everything NOT `in: body` stays as parameters,
/// the `in: body` parameter becomes the request body.
fn split_swagger2_params(params: &[Value]) -> (Vec<Value>, Option<Value>) {
    let mut query_params = Vec::new();
    let mut body_param = None;

    for param in params {
        let is_body = param
            .get("in")
            .and_then(|v| v.as_str())
            .map(|s| s == "body")
            .unwrap_or(false);

        if is_body {
            body_param = Some(param.clone());
        } else {
            query_params.push(param.clone());
        }
    }

    (query_params, body_param)
}

/// Convert a Swagger 2.0 `in: body` parameter to OpenAPI 3.0 requestBody
fn convert_body_param_to_request_body(body_param: &Value) -> Value {
    let schema = body_param
        .get("schema")
        .cloned()
        .unwrap_or(json!({"type": "object"}));
    let description = body_param
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let required = body_param
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    json!({
        "description": description,
        "required": required,
        "content": {
            "application/json": {
                "schema": schema
            }
        }
    })
}

/// Build content map for response `produces` mimetypes
fn build_content_for_produces(produces: &[Value], response: &Value) -> Value {
    let mut content = Map::new();

    // Try to find a schema from the response
    let schema = response
        .get("schema")
        .cloned()
        .unwrap_or(json!({"type": "object"}));

    for mime in produces {
        if let Some(mime_str) = mime.as_str() {
            content.insert(mime_str.to_string(), json!({"schema": schema}));
        }
    }

    // If no produces specified, default to application/json
    if content.is_empty() {
        content.insert("application/json".to_string(), json!({"schema": schema}));
    }

    Value::Object(content)
}

/// Convert Swagger 2.0 definitions to OpenAPI 3.0 schemas
fn convert_swagger2_definitions(defs: &Map<String, Value>) -> Result<Map<String, Value>> {
    // Definitions are mostly compatible — just need to strip Swagger 2.0-specific
    // fields like `discriminator` (which moved in 3.0)
    let mut out = Map::new();
    for (name, def) in defs {
        out.insert(name.clone(), def.clone());
    }
    Ok(out)
}

// ── OpenAPI 3.1 → 3.0 Downgrade ───────────────────────────────────────────

fn downgrade_openapi31_to_30(spec: &Value) -> Result<Value> {
    let mut out = spec.clone();

    // Change version string
    if let Some(obj) = out.as_object_mut() {
        obj.insert("openapi".into(), json!("3.0.3"));

        // Remove 3.1-specific fields
        obj.remove("jsonSchemaDialect");
        obj.remove("webhooks");

        // Walk through the spec and fix `type` arrays (3.1 allows `type: ["string", "null"]`)
        if let Some(components) = obj.get_mut("components").and_then(|c| c.as_object_mut()) {
            if let Some(schemas) = components
                .get_mut("schemas")
                .and_then(|s| s.as_object_mut())
            {
                for (_name, schema) in schemas.iter_mut() {
                    fix_type_arrays_for_30(schema);
                }
            }
        }

        // Walk paths
        if let Some(paths) = obj.get_mut("paths").and_then(|p| p.as_object_mut()) {
            for (_path, path_item) in paths.iter_mut() {
                fix_type_arrays_in_value(path_item);
            }
        }
    }

    debug!("OpenAPI 3.1 → 3.0 downgrade complete");

    Ok(out)
}

/// Fix `type: ["string", "null"]` → `type: "string", nullable: true`
fn fix_type_arrays_for_30(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        // Fix type array
        if let Some(types) = obj.get("type").and_then(|v| v.as_array()) {
            let non_null: Vec<&Value> = types
                .iter()
                .filter(|t| t.as_str() != Some("null"))
                .collect();
            if non_null.len() == 1 {
                obj.insert("type".into(), non_null[0].clone());
                obj.insert("nullable".into(), json!(true));
            } else if non_null.is_empty() {
                obj.remove("type");
                obj.insert("nullable".into(), json!(true));
            }
        }

        // Recurse into nested schemas
        if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
            for (_name, prop) in props.iter_mut() {
                fix_type_arrays_for_30(prop);
            }
        }
        if let Some(items) = obj.get_mut("items") {
            fix_type_arrays_for_30(items);
        }
        if let Some(additional) = obj.get_mut("additionalProperties") {
            fix_type_arrays_for_30(additional);
        }
        if let Some(all_of) = obj.get_mut("allOf").and_then(|a| a.as_array_mut()) {
            for item in all_of {
                fix_type_arrays_for_30(item);
            }
        }
    }
}

fn fix_type_arrays_in_value(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        for (_key, val) in obj.iter_mut() {
            fix_type_arrays_for_30(val);
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn is_http_method(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_swagger2() {
        let spec = json!({"swagger": "2.0", "info": {"title": "Test", "version": "1.0"}});
        assert_eq!(detect_version(&spec), SpecVersion::Swagger2);
    }

    #[test]
    fn test_detect_openapi30() {
        let spec = json!({"openapi": "3.0.3", "info": {"title": "Test", "version": "1.0"}});
        assert_eq!(detect_version(&spec), SpecVersion::OpenApi30);
    }

    #[test]
    fn test_detect_openapi31() {
        let spec = json!({"openapi": "3.1.0", "info": {"title": "Test", "version": "1.0"}});
        assert_eq!(detect_version(&spec), SpecVersion::OpenApi31);
    }

    #[test]
    fn test_normalize_openapi30_passthrough() {
        let spec = json!({
            "openapi": "3.0.3",
            "info": {"title": "Test", "version": "1.0"},
            "paths": {}
        });
        let result = normalize(&spec).unwrap();
        assert_eq!(result["openapi"], "3.0.3");
    }

    #[test]
    fn test_convert_swagger2_basic() {
        let spec = json!({
            "swagger": "2.0",
            "info": {"title": "Pet Store", "version": "1.0.0"},
            "host": "petstore.example.com",
            "basePath": "/api",
            "schemes": ["https"],
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "summary": "List all pets",
                        "responses": {
                            "200": {"description": "A list of pets"}
                        }
                    }
                }
            }
        });

        let result = normalize(&spec).unwrap();
        assert_eq!(result["openapi"], "3.0.3");
        assert_eq!(result["info"]["title"], "Pet Store");

        // Check server URL
        let servers = result["servers"].as_array().unwrap();
        assert_eq!(servers[0]["url"], "https://petstore.example.com/api");

        // Check paths still work
        assert!(result["paths"]["/pets"]["get"].is_object());
    }

    #[test]
    fn test_convert_swagger2_definitions_to_schemas() {
        let spec = json!({
            "swagger": "2.0",
            "info": {"title": "Test", "version": "1.0"},
            "paths": {},
            "definitions": {
                "Pet": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });

        let result = normalize(&spec).unwrap();
        let schemas = &result["components"]["schemas"];
        assert!(schemas["Pet"].is_object());
        assert_eq!(schemas["Pet"]["properties"]["name"]["type"], "string");
    }

    #[test]
    fn test_convert_swagger2_body_param() {
        let spec = json!({
            "swagger": "2.0",
            "info": {"title": "Test", "version": "1.0"},
            "paths": {
                "/pets": {
                    "post": {
                        "operationId": "createPet",
                        "parameters": [
                            {"in": "body", "name": "body", "schema": {"type": "object"}}
                        ],
                        "responses": {"201": {"description": "Created"}}
                    }
                }
            }
        });

        let result = normalize(&spec).unwrap();
        let post = &result["paths"]["/pets"]["post"];

        // Body param should be converted to requestBody
        assert!(post["requestBody"].is_object());
        // Parameters should be empty or absent (body param removed)
        let params = post["parameters"].as_array().map(|a| a.len()).unwrap_or(0);
        assert_eq!(params, 0, "Body-only operation should have no query params");
    }

    #[test]
    fn test_downgrade_openapi31_type_arrays() {
        let spec = json!({
            "openapi": "3.1.0",
            "info": {"title": "Test", "version": "1.0"},
            "paths": {},
            "components": {
                "schemas": {
                    "User": {
                        "type": "object",
                        "properties": {
                            "email": {"type": ["string", "null"]},
                            "age": {"type": ["integer", "null"]}
                        }
                    }
                }
            }
        });

        let result = normalize(&spec).unwrap();
        assert_eq!(result["openapi"], "3.0.3");

        let email = &result["components"]["schemas"]["User"]["properties"]["email"];
        assert_eq!(email["type"], "string");
        assert_eq!(email["nullable"], true);

        let age = &result["components"]["schemas"]["User"]["properties"]["age"];
        assert_eq!(age["type"], "integer");
        assert_eq!(age["nullable"], true);
    }

    #[test]
    fn test_downgrade_strips_json_schema_dialect() {
        let spec = json!({
            "openapi": "3.1.0",
            "jsonSchemaDialect": "https://spec.openapis.org/oas/3.1/dialect/base",
            "info": {"title": "Test", "version": "1.0"},
            "paths": {}
        });

        let result = normalize(&spec).unwrap();
        assert!(result.get("jsonSchemaDialect").is_none());
    }

    #[test]
    fn test_is_http_method() {
        assert!(is_http_method("GET"));
        assert!(is_http_method("post"));
        assert!(is_http_method("DELETE"));
        assert!(is_http_method("PATCH"));
        assert!(!is_http_method("parameters"));
        assert!(!is_http_method("summary"));
    }
}
