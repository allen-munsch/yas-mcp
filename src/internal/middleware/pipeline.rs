//! Response pipeline — compose, transform, and aggregate API responses.
//!
//! Pipelines solve the common problems orgs face when consuming REST APIs through MCP:
//!
//! 1. **Pagination**: Auto-fetch all pages, combine into a single result
//! 2. **Joins**: Call API B with results from API A, merge responses
//! 3. **Filtering**: Keep/drop fields from responses
//! 4. **Mapping**: Rename, transform, compute new fields
//! 5. **Aggregation**: Count, sum, group-by across paginated results
//! 6. **Caching**: Cache responses with TTL, serve from cache on repeat calls
//!
//! ## Declarative YAML
//!
//! ```yaml
//! pipelines:
//!   enriched-users:
//!     stages:
//!       - paginate:
//!           strategy: cursor
//!           cursor_path: $.next_cursor
//!           results_path: $.data
//!           max_pages: 50
//!       - join:
//!           tool: getDepartment
//!           on:
//!             local: department_id
//!             remote: id
//!           as: department
//!       - filter:
//!           rules:
//!             - field: status
//!               op: eq
//!               value: active
//!       - map:
//!           fields:
//!             id: $.id
//!             name: $.name
//!             email: $.email
//!             dept_name: $.department.name
//!       - aggregate:
//!           group_by: department.name
//!           metrics:
//!             - count: id
//!             - avg: salary
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// ──────────────────────────────────────────────
//  Stage types and configuration
// ──────────────────────────────────────────────

/// The type of pipeline stage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StageType {
    /// Auto-paginate through all pages, collect results
    Paginate,
    /// Call another tool and merge its result
    Join,
    /// Filter items by field conditions
    Filter,
    /// Map/rename/transform fields
    Map,
    /// Aggregate metrics across items
    Aggregate,
    /// Cache the stage output
    Cache,
}

impl fmt::Display for StageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageType::Paginate => write!(f, "paginate"),
            StageType::Join => write!(f, "join"),
            StageType::Filter => write!(f, "filter"),
            StageType::Map => write!(f, "map"),
            StageType::Aggregate => write!(f, "aggregate"),
            StageType::Cache => write!(f, "cache"),
        }
    }
}

/// Pagination strategy for the paginate stage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaginationStrategy {
    /// Cursor-based: response contains a next_cursor/page_token for the next page
    Cursor,
    /// Page-number based: ?page=N
    PageNumber,
    /// Offset-limit based: ?offset=N&limit=M
    OffsetLimit,
    /// Link-header based: RFC 5988 Link header
    LinkHeader,
}

/// Configuration for a single paginate stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginateConfig {
    /// Pagination strategy
    pub strategy: PaginationStrategy,

    /// JSONPath to the next cursor/page token in the response
    #[serde(default)]
    pub cursor_path: Option<String>,

    /// JSONPath to the array of results in the response
    #[serde(default = "default_results_path")]
    pub results_path: String,

    /// Query parameter name for page number (page_number strategy)
    #[serde(default = "default_page_param")]
    pub page_param: String,

    /// Query parameter name for offset (offset_limit strategy)
    #[serde(default)]
    pub offset_param: Option<String>,

    /// Query parameter name for limit
    #[serde(default = "default_limit_param")]
    pub limit_param: String,

    /// Page size / limit per request
    #[serde(default = "default_page_size")]
    pub page_size: u32,

    /// Maximum number of pages to fetch (safety limit)
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,

    /// Maximum total results (safety limit)
    #[serde(default = "default_max_results")]
    pub max_results: u32,

    /// Delay between page requests (for rate-limited APIs)
    #[serde(default)]
    pub delay_ms: u64,
}

fn default_results_path() -> String {
    "$.data".to_string()
}
fn default_page_param() -> String {
    "page".to_string()
}
fn default_limit_param() -> String {
    "limit".to_string()
}
fn default_page_size() -> u32 {
    100
}
fn default_max_pages() -> u32 {
    20
}
fn default_max_results() -> u32 {
    10000
}

impl Default for PaginateConfig {
    fn default() -> Self {
        Self {
            strategy: PaginationStrategy::Cursor,
            cursor_path: Some("$.next_cursor".to_string()),
            results_path: default_results_path(),
            page_param: default_page_param(),
            offset_param: None,
            limit_param: default_limit_param(),
            page_size: default_page_size(),
            max_pages: default_max_pages(),
            max_results: default_max_results(),
            delay_ms: 0,
        }
    }
}

/// Configuration for a join stage — call another MCP tool and merge results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinConfig {
    /// Name of the MCP tool to call for joined data
    pub tool: String,

    /// Join condition: local field → remote parameter
    pub on: JoinCondition,

    /// Nest joined result under this field name
    #[serde(rename = "as")]
    pub as_field: String,

    /// Strategy: one-to-one (default) or one-to-many
    #[serde(default)]
    pub strategy: JoinStrategy,

    /// Max concurrent join requests
    #[serde(default = "default_join_concurrency")]
    pub concurrency: u32,

    /// Skip join if local field is null/missing
    #[serde(default = "default_true_bool")]
    pub skip_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinCondition {
    /// Field in the local (current pipeline) item
    pub local: String,

    /// Parameter name for the remote tool call
    pub remote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JoinStrategy {
    /// Each local item joins to exactly one remote item
    OneToOne,
    /// Each local item joins to multiple remote items (array)
    OneToMany,
}

impl Default for JoinStrategy {
    fn default() -> Self {
        JoinStrategy::OneToOne
    }
}

fn default_join_concurrency() -> u32 {
    5
}
fn default_true_bool() -> bool {
    true
}

/// Filter rule for the filter stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRule {
    /// Field path (supports dot notation: "department.name")
    pub field: String,

    /// Comparison operator
    pub op: FilterOp,

    /// Value to compare against
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    NotIn,
    Contains,
    StartsWith,
    EndsWith,
    Exists,
    NotExists,
    Regex,
}

/// Configuration for a filter stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Filter rules — ALL must match (AND logic)
    pub rules: Vec<FilterRule>,

    /// Logic: "and" (all rules must match) or "or" (any rule matches)
    #[serde(default = "default_logic")]
    pub logic: FilterLogic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterLogic {
    And,
    Or,
}

fn default_logic() -> FilterLogic {
    FilterLogic::And
}

/// Field mapping for the map stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMap {
    /// New field name
    pub name: String,

    /// Source value: static string, JSONPath expression, or template
    #[serde(flatten)]
    pub source: FieldSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldSource {
    /// Static value
    Static {
        value: serde_json::Value,
    },
    /// JSONPath expression
    Path {
        path: String,
    },
    /// Template with {placeholders}
    Template {
        template: String,
    },
}

/// Configuration for a map stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapConfig {
    /// Fields to keep (whitelist — if specified, only these survive)
    #[serde(default)]
    pub keep: Vec<String>,

    /// Fields to drop (blacklist)
    #[serde(default)]
    pub drop: Vec<String>,

    /// New/renamed/computed fields
    #[serde(default)]
    pub fields: Vec<FieldMapConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMapConfig {
    /// Target field name
    pub name: String,

    /// Source JSONPath (from current item)
    pub path: Option<String>,

    /// Static value
    pub value: Option<serde_json::Value>,

    /// Template: "Hello {name}, your id is {id}"
    pub template: Option<String>,
}

/// Configuration for an aggregate stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateConfig {
    /// Field to group by (optional — omit for global aggregate)
    #[serde(default)]
    pub group_by: Option<String>,

    /// Metrics to compute
    pub metrics: Vec<AggregateMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetric {
    /// Metric type
    #[serde(rename = "type")]
    pub metric_type: AggregateType,

    /// Field to compute metric on
    pub field: String,

    /// Output field name for the result
    #[serde(rename = "as")]
    pub as_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregateType {
    Count,
    CountDistinct,
    Sum,
    Avg,
    Min,
    Max,
}

/// Configuration for a cache stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// TTL in seconds
    pub ttl: u64,

    /// Cache key template (uses tool name + args by default)
    #[serde(default)]
    pub key_template: Option<String>,
}

// ──────────────────────────────────────────────
//  Stage
// ──────────────────────────────────────────────

/// A single stage in a pipeline, with its type and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PipelineStage {
    #[serde(rename = "paginate")]
    Paginate(PaginateConfig),
    #[serde(rename = "join")]
    Join(JoinConfig),
    #[serde(rename = "filter")]
    Filter(FilterConfig),
    #[serde(rename = "map")]
    Map(MapConfig),
    #[serde(rename = "aggregate")]
    Aggregate(AggregateConfig),
    #[serde(rename = "cache")]
    Cache(CacheConfig),
}

impl PipelineStage {
    pub fn stage_type(&self) -> StageType {
        match self {
            PipelineStage::Paginate(_) => StageType::Paginate,
            PipelineStage::Join(_) => StageType::Join,
            PipelineStage::Filter(_) => StageType::Filter,
            PipelineStage::Map(_) => StageType::Map,
            PipelineStage::Aggregate(_) => StageType::Aggregate,
            PipelineStage::Cache(_) => StageType::Cache,
        }
    }
}

// ──────────────────────────────────────────────
//  Stage output
// ──────────────────────────────────────────────

/// The data flowing through the pipeline.
/// Each stage consumes this and produces a new one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutput {
    /// The processed items (after filtering, mapping, etc.)
    pub items: serde_json::Value,

    /// Metadata about what happened
    pub metadata: StageMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMetadata {
    /// Number of items in this output
    pub item_count: usize,

    /// Number of upstream API calls made
    pub api_calls: u32,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// Stage-specific details
    #[serde(default)]
    pub details: HashMap<String, serde_json::Value>,
}

impl StageOutput {
    /// Create from an array
    pub fn from_array(items: Vec<serde_json::Value>) -> Self {
        let count = items.len();
        Self {
            items: serde_json::Value::Array(items),
            metadata: StageMetadata {
                item_count: count,
                api_calls: 0,
                bytes_processed: 0,
                details: HashMap::new(),
            },
        }
    }

    /// Wrap a single value as an item
    pub fn from_value(value: serde_json::Value) -> Self {
        let item_count = if value.is_array() {
            value.as_array().map(|a| a.len()).unwrap_or(0)
        } else {
            1
        };
        Self {
            items: value,
            metadata: StageMetadata {
                item_count,
                api_calls: 0,
                bytes_processed: 0,
                details: HashMap::new(),
            },
        }
    }
}

// ──────────────────────────────────────────────
//  Pipeline
// ──────────────────────────────────────────────

/// A named pipeline composed of stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Pipeline name (referenced in tool configuration)
    pub name: String,

    /// Description
    #[serde(default)]
    pub description: String,

    /// Stages to execute in order
    pub stages: Vec<PipelineStage>,
}

/// Runtime pipeline reference — resolved and ready to execute.
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub name: String,
    pub stages: Vec<PipelineStage>,
}

impl Pipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            name: config.name,
            stages: config.stages,
        }
    }
}

// ──────────────────────────────────────────────
//  Pipeline registry
// ──────────────────────────────────────────────

/// Registry of all configured pipelines, keyed by name.
#[derive(Debug, Default)]
pub struct PipelineRegistry {
    pipelines: HashMap<String, Arc<Pipeline>>,
    /// Map of tool_name → pipeline_name
    tool_bindings: HashMap<String, String>,
}

impl PipelineRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            tool_bindings: HashMap::new(),
        }
    }

    /// Register a pipeline
    pub fn register(&mut self, config: PipelineConfig) {
        let name = config.name.clone();
        self.pipelines.insert(name, Arc::new(Pipeline::new(config)));
    }

    /// Bind a pipeline to a specific MCP tool
    pub fn bind_tool(&mut self, tool_name: &str, pipeline_name: &str) {
        self.tool_bindings
            .insert(tool_name.to_string(), pipeline_name.to_string());
    }

    /// Resolve pipeline for a tool call
    pub fn resolve_for_tool(&self, tool_name: &str) -> Option<Arc<Pipeline>> {
        let pipeline_name = self.tool_bindings.get(tool_name)?;
        self.pipelines.get(pipeline_name).cloned()
    }

    /// Get a pipeline by name
    pub fn get(&self, name: &str) -> Option<Arc<Pipeline>> {
        self.pipelines.get(name).cloned()
    }

    /// List all registered pipeline names
    pub fn list_names(&self) -> Vec<String> {
        self.pipelines.keys().cloned().collect()
    }
}

// ──────────────────────────────────────────────
//  Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_paginate_stage() {
        let yaml = r#"
        name: test-pipeline
        stages:
          - type: paginate
            strategy: cursor
            cursor_path: $.next
            results_path: $.items
            max_pages: 10
        "#;

        let config: PipelineConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "test-pipeline");
        assert_eq!(config.stages.len(), 1);

        match &config.stages[0] {
            PipelineStage::Paginate(p) => {
                assert_eq!(p.strategy, PaginationStrategy::Cursor);
                assert_eq!(p.max_pages, 10);
                assert_eq!(p.results_path, "$.items");
            }
            _ => panic!("Expected paginate stage"),
        }
    }

    #[test]
    fn test_parse_full_pipeline() {
        let yaml = r#"
        name: enriched-users
        stages:
          - type: paginate
            strategy: page_number
            results_path: $.data
            page_size: 50
            max_pages: 10
          - type: join
            tool: getDepartment
            on:
              local: department_id
              remote: id
            as: department
          - type: filter
            rules:
              - field: status
                op: eq
                value: active
          - type: map
            keep:
              - id
              - name
            fields:
              - name: email
                path: $.contact.email
          - type: aggregate
            group_by: department.name
            metrics:
              - type: count
                field: id
                as: total_users
        "#;

        let config: PipelineConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "enriched-users");
        assert_eq!(config.stages.len(), 5);
    }

    #[test]
    fn test_pipeline_registry() {
        let mut registry = PipelineRegistry::new();

        let config = PipelineConfig {
            name: "enrich".into(),
            description: "".into(),
            stages: vec![
                PipelineStage::Paginate(PaginateConfig::default()),
                PipelineStage::Map(MapConfig {
                    keep: vec!["id".into(), "name".into()],
                    drop: vec![],
                    fields: vec![],
                }),
            ],
        };

        registry.register(config);
        registry.bind_tool("listUsers", "enrich");

        assert!(registry.resolve_for_tool("listUsers").is_some());
        assert!(registry.resolve_for_tool("unknown").is_none());
        assert_eq!(registry.list_names(), vec!["enrich"]);
    }

    #[test]
    fn test_stage_output_from_array() {
        let items = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
        ];
        let output = StageOutput::from_array(items);
        assert_eq!(output.metadata.item_count, 2);
        assert!(output.items.is_array());
    }

    #[test]
    fn test_filter_rule_parsing() {
        let yaml = r#"
        field: age
        op: gte
        value: 18
        "#;
        let rule: FilterRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.field, "age");
        assert_eq!(rule.op, FilterOp::Gte);
        assert_eq!(rule.value, serde_json::json!(18));
    }

    #[test]
    fn test_aggregate_metric_parsing() {
        let yaml = r#"
        type: avg
        field: salary
        as: avg_salary
        "#;
        let metric: AggregateMetric = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(metric.metric_type, AggregateType::Avg);
        assert_eq!(metric.field, "salary");
        assert_eq!(metric.as_field, "avg_salary");
    }
}
