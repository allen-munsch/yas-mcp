//! Mock Mode — Run without an upstream API
//!
//! Generates realistic fake responses from OpenAPI schemas.
//! Useful for development, testing, and demos.
//!
//! # How it works
//!
//! 1. Parse the OpenAPI spec to extract response schemas
//! 2. For each route, generate a mock response from the schema:
//!    - `string` → example value or random word
//!    - `integer` → example value or random number in range
//!    - `boolean` → example value or `false`
//!    - `array` → 1-3 items from the items schema
//!    - `object` → populate all properties
//!    - `$ref` → follow reference
//! 3. Return the mock response directly, no upstream call needed
//!
//! # Usage
//!
//! ```bash
//! yas-mcp --swagger-file api.yaml --mock
//! ```
//!
//! Or in config:
//! ```yaml
//! endpoint:
//!   mock: true
//! ```

use rand::Rng;
use serde_json::{Map, Value, json};
use tracing::debug;

/// Generate a mock response for a response schema
pub fn generate_mock_from_schema(schema: &Value) -> Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => mock_string(schema),
        Some("integer") | Some("number") => mock_number(schema),
        Some("boolean") => mock_boolean(schema),
        Some("array") => mock_array(schema),
        Some("object") => mock_object(schema),
        _ => {
            // Check for enum
            if let Some(enums) = schema.get("enum").and_then(|e| e.as_array())
                && let Some(first) = enums.first() {
                    return first.clone();
                }
            // Check for oneOf/anyOf — pick first option
            if let Some(one_of) = schema
                .get("oneOf")
                .or(schema.get("anyOf"))
                .and_then(|a| a.as_array())
                && let Some(first) = one_of.first() {
                    return generate_mock_from_schema(first);
                }
            // Fallback
            json!("mock_value")
        }
    }
}

fn mock_string(schema: &Value) -> Value {
    // Prefer example
    if let Some(ex) = schema.get("example").and_then(|v| v.as_str()) {
        return json!(ex);
    }
    // Try format hints
    match schema.get("format").and_then(|f| f.as_str()) {
        Some("email") => json!("user@example.com"),
        Some("uri") | Some("url") => json!("https://example.com"),
        Some("date") => json!("2025-01-15"),
        Some("date-time") => json!("2025-01-15T10:30:00Z"),
        Some("uuid") => json!("550e8400-e29b-41d4-a716-446655440000"),
        Some("hostname") => json!("example.com"),
        Some("ipv4") => json!("192.168.1.1"),
        Some("ipv6") => json!("::1"),
        _ => {
            let words = [
                "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
            ];
            let i = rand::thread_rng().gen_range(0..words.len());
            json!(words[i])
        }
    }
}

fn mock_number(schema: &Value) -> Value {
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    let mut rng = rand::thread_rng();
    let min = schema
        .get("minimum")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let max = schema
        .get("maximum")
        .and_then(|v| v.as_f64())
        .unwrap_or(100.0);
    let val = rng.gen_range(min..max);
    if schema.get("type").and_then(|t| t.as_str()) == Some("integer") {
        json!(val as i64)
    } else {
        json!((val * 100.0).round() / 100.0)
    }
}

fn mock_boolean(schema: &Value) -> Value {
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    json!(false)
}

fn mock_array(schema: &Value) -> Value {
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    let items = schema
        .get("items")
        .cloned()
        .unwrap_or(json!({"type": "string"}));
    let min = schema.get("minItems").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let max = schema.get("maxItems").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
    let count = if min < max {
        rand::thread_rng().gen_range(min..=max)
    } else {
        min
    };

    let arr: Vec<Value> = (0..count)
        .map(|_| generate_mock_from_schema(&items))
        .collect();
    json!(arr)
}

fn mock_object(schema: &Value) -> Value {
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }

    let mut obj = Map::new();

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, prop_schema) in props {
            obj.insert(name.clone(), generate_mock_from_schema(prop_schema));
        }
    }

    // If no properties, return empty object
    if obj.is_empty() {
        obj.insert("id".into(), json!(1));
        obj.insert("name".into(), json!("mock_item"));
    }

    json!(obj)
}

/// Generate a mock HTTP response for a given path and method from an OpenAPI spec.
/// Returns (status_code, response_body_json).
pub fn mock_response_for_route(spec: &Value, path: &str, method: &str) -> Option<(u16, Value)> {
    let path_item = spec.get("paths")?.get(path)?;

    // Handle path parameters — try exact match first, then template match
    let path_item = if path_item.is_object() {
        path_item
    } else {
        // Try matching against templated paths like /users/{id}
        let paths = spec.get("paths")?.as_object()?;
        for (template, _item) in paths {
            if path_matches_template(path, template) {
                return mock_response_for_route(spec, template, method);
            }
        }
        return None;
    };

    let operation = path_item.get(method.to_lowercase())?;
    let responses = operation.get("responses")?;

    // Prefer 200, then 201, then first available
    let status_code = if responses.get("200").is_some() {
        200
    } else if responses.get("201").is_some() {
        201
    } else if let Some(first_key) = responses.as_object()?.keys().next() {
        first_key.parse().unwrap_or(200)
    } else {
        200
    };

    let status_str = status_code.to_string();
    let response_def = responses
        .get(&status_str)
        .or_else(|| responses.get("default"))?;

    // Extract schema from response content
    let schema = response_def
        .get("content")
        .and_then(|c| c.get("application/json"))
        .and_then(|c| c.get("schema"))
        .or_else(|| response_def.get("schema"));

    let body = match schema {
        Some(s) => generate_mock_from_schema(s),
        None => json!({"message": "mock response"}),
    };

    debug!("Mock: {method} {path} → {status_code}");

    Some((status_code, body))
}

/// Simple path template matching: /users/{id} matches /users/42
fn path_matches_template(actual: &str, template: &str) -> bool {
    let actual_segments: Vec<&str> = actual.split('/').filter(|s| !s.is_empty()).collect();
    let template_segments: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();

    if actual_segments.len() != template_segments.len() {
        return false;
    }

    actual_segments
        .iter()
        .zip(template_segments.iter())
        .all(|(a, t)| t.starts_with('{') || a == t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_string_with_example() {
        let schema = json!({"type": "string", "example": "hello"});
        assert_eq!(generate_mock_from_schema(&schema), "hello");
    }

    #[test]
    fn test_mock_email_format() {
        let schema = json!({"type": "string", "format": "email"});
        let result = generate_mock_from_schema(&schema);
        assert!(result.as_str().unwrap().contains('@'));
    }

    #[test]
    fn test_mock_integer_with_range() {
        let schema = json!({"type": "integer", "minimum": 10, "maximum": 20});
        let result = generate_mock_from_schema(&schema);
        let val = result.as_i64().unwrap();
        assert!(val >= 10 && val <= 20);
    }

    #[test]
    fn test_mock_boolean() {
        let schema = json!({"type": "boolean"});
        let result = generate_mock_from_schema(&schema);
        assert!(result.is_boolean());
    }

    #[test]
    fn test_mock_array() {
        let schema = json!({
            "type": "array",
            "items": {"type": "string", "example": "item"},
            "minItems": 1,
            "maxItems": 3
        });
        let result = generate_mock_from_schema(&schema);
        let arr = result.as_array().unwrap();
        assert!(arr.len() >= 1 && arr.len() <= 3);
    }

    #[test]
    fn test_mock_object_with_properties() {
        let schema = json!({
            "type": "object",
            "properties": {
                "id": {"type": "integer", "example": 42},
                "name": {"type": "string", "example": "test"},
                "active": {"type": "boolean"}
            }
        });
        let result = generate_mock_from_schema(&schema);
        assert_eq!(result["id"], 42);
        assert_eq!(result["name"], "test");
        assert!(result["active"].is_boolean());
    }

    #[test]
    fn test_mock_enum() {
        let schema = json!({"enum": ["red", "green", "blue"]});
        assert_eq!(generate_mock_from_schema(&schema), "red");
    }

    #[test]
    fn test_mock_one_of() {
        let schema = json!({
            "oneOf": [
                {"type": "string", "example": "first"},
                {"type": "integer", "example": 42}
            ]
        });
        let result = generate_mock_from_schema(&schema);
        assert_eq!(result, "first");
    }

    #[test]
    fn test_path_matches_template() {
        assert!(path_matches_template("/users/42", "/users/{id}"));
        assert!(path_matches_template(
            "/orgs/acme/repos/123",
            "/orgs/{org}/repos/{repo}"
        ));
        assert!(!path_matches_template("/users/42/posts", "/users/{id}"));
        assert!(!path_matches_template("/other/42", "/users/{id}"));
    }

    #[test]
    fn test_mock_response_for_route() {
        let spec = json!({
            "openapi": "3.0.0",
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "responses": {
                            "200": {
                                "description": "List of pets",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "array",
                                            "items": {
                                                "type": "object",
                                                "properties": {
                                                    "id": {"type": "integer"},
                                                    "name": {"type": "string"}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let (status, body) = mock_response_for_route(&spec, "/pets", "GET").unwrap();
        assert_eq!(status, 200);
        assert!(body.is_array());
    }

    #[test]
    fn test_mock_response_nonexistent_route() {
        let spec = json!({"openapi": "3.0.0", "paths": {}});
        assert!(mock_response_for_route(&spec, "/nonexistent", "GET").is_none());
    }
}
