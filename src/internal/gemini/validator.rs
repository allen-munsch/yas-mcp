/// Validates tools and schemas for Gemini CLI compatibility
pub struct GeminiValidator;

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub tool_name: String,
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl GeminiValidator {
    /// Validate a tool for Gemini compatibility
    pub fn validate_tool(tool: &rmcp::model::Tool) -> ValidationResult {
        let mut result = ValidationResult {
            tool_name: tool.name.to_string(),
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        // Rule 1: Tool name must be <= 64 characters
        if tool.name.len() > 64 {
            result.errors.push(format!(
                "Tool name exceeds 64 characters ({} chars): '{}'",
                tool.name.len(),
                tool.name
            ));
            result.is_valid = false;
        }

        // Rule 2: Tool name must match pattern [a-zA-Z_][a-zA-Z0-9_]*
        let name_regex = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();
        if !name_regex.is_match(&tool.name) {
            result.errors.push(format!(
                "Tool name contains invalid characters: '{}'",
                tool.name
            ));
            result.is_valid = false;
        }

        // Rule 3: Check JSON schema for unsupported constructs
        let schema_issues = Self::validate_schema(&tool.input_schema);
        for issue in schema_issues {
            if issue.is_error {
                result.errors.push(issue.message);
                result.is_valid = false;
            } else {
                result.warnings.push(issue.message);
            }
        }

        // Rule 4: Output schema must be an Object (fixes your specific CLI error)
        if let Some(schema) = &tool.output_schema {
            if let Some(type_val) = schema.get("type") {
                if type_val != "object" {
                    result.errors.push(format!(
                        "Output schema type must be 'object', found '{}'",
                        type_val
                    ));
                    result.is_valid = false;
                }
            }
        }

        result
    }

    /// Validate JSON Schema for Gemini compatibility
    fn validate_schema(schema: &serde_json::Map<String, serde_json::Value>) -> Vec<SchemaIssue> {
        let mut issues = Vec::new();

        // Check for unsupported keywords
        let unsupported = ["oneOf", "anyOf", "allOf", "$ref", "additionalProperties"];
        for keyword in unsupported {
            if schema.contains_key(keyword) {
                issues.push(SchemaIssue {
                    message: format!("Schema contains unsupported keyword '{}'", keyword),
                    is_error: true,
                });
            }
        }

        // Recursively check nested schemas
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_schema) in properties {
                if let Some(prop_obj) = prop_schema.as_object() {
                    let nested_issues = Self::validate_schema(prop_obj);
                    for mut issue in nested_issues {
                        issue.message = format!("In property '{}': {}", prop_name, issue.message);
                        issues.push(issue);
                    }
                }
            }
        }

        issues
    }

    /// Validate all tools and return report
    pub fn validate_all(tools: &[rmcp::model::Tool]) -> GeminiCompatibilityReport {
        let results: Vec<_> = tools.iter().map(Self::validate_tool).collect();

        let valid_count = results.iter().filter(|r| r.is_valid).count();
        let invalid_count = results.len() - valid_count;

        GeminiCompatibilityReport {
            total_tools: results.len(),
            valid_tools: valid_count,
            invalid_tools: invalid_count,
            results,
        }
    }
}

#[derive(Debug)]
struct SchemaIssue {
    message: String,
    is_error: bool,
}

#[derive(Debug)]
pub struct GeminiCompatibilityReport {
    pub total_tools: usize,
    pub valid_tools: usize,
    pub invalid_tools: usize,
    pub results: Vec<ValidationResult>,
}
