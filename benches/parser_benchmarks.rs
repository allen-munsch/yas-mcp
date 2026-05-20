//! Parser Benchmarks
//!
//! Criterion benchmarks for critical paths:
//! - OpenAPI spec parsing
//! - Tool registration
//! - MCP request routing

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yas_mcp::internal::parser::SwaggerParser;
use yas_mcp::internal::parser::adjuster::Adjuster;
use yas_mcp::internal::parser::types::Parser;

fn bench_parse_todo_spec(c: &mut Criterion) {
    c.bench_function("parse todo-app openapi", |b| {
        b.iter(|| {
            let mut parser = SwaggerParser::new(Adjuster::new());
            parser
                .init("examples/todo-app/openapi.yaml", None)
                .unwrap();
            let tools = parser.get_route_tools().len();
            black_box(tools)
        })
    });
}

fn bench_parse_petstore(c: &mut Criterion) {
    c.bench_function("parse petstore openapi", |b| {
        b.iter(|| {
            let mut parser = SwaggerParser::new(Adjuster::new());
            parser.init("examples/petstore.yaml", None).unwrap();
            let tools = parser.get_route_tools().len();
            black_box(tools)
        })
    });
}

fn bench_parse_prefect_large(c: &mut Criterion) {
    c.bench_function("parse prefect-oas-3.6.1 (large)", |b| {
        b.iter(|| {
            let mut parser = SwaggerParser::new(Adjuster::new());
            parser
                .init("examples/prefect-oas-3.6.1.json", Some("adjustments.yaml"))
                .unwrap();
            let tools = parser.get_route_tools().len();
            black_box(tools)
        })
    });
}

fn bench_tool_registration(c: &mut Criterion) {
    use std::sync::Arc;
    use yas_mcp::internal::mcp::registry::{RegisteredTool, ToolRegistry};
    use yas_mcp::internal::server::tool::handler::ToolExecutor;
    use rmcp::model::Tool;

    c.bench_function("register 50 tools", |b| {
        b.iter(|| {
            let registry = Arc::new(ToolRegistry::new());
            for i in 0..50 {
                let tool = Tool {
                    name: format!("tool_{i}").into(),
                    title: None,
                    description: Some(format!("Tool {i}").into()),
                    input_schema: Arc::new(serde_json::Map::new()),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                };
                let executor: ToolExecutor = Arc::new(|_req| {
                    Box::pin(async {
                        Ok(rmcp::model::CallToolResult {
                            content: vec![],
                            is_error: Some(false),
                            meta: None,
                            structured_content: None,
                        })
                    })
                });
                registry.register(
                    format!("tool_{i}"),
                    RegisteredTool {
                        metadata: tool,
                        executor,
                    },
                );
            }
            black_box(registry.count())
        })
    });
}

fn bench_tool_lookup(c: &mut Criterion) {
    use std::sync::Arc;
    use yas_mcp::internal::mcp::registry::{RegisteredTool, ToolRegistry};
    use yas_mcp::internal::server::tool::handler::ToolExecutor;
    use rmcp::model::Tool;

    let registry = Arc::new(ToolRegistry::new());
    for i in 0..100 {
        let tool = Tool {
            name: format!("tool_{i}").into(),
            title: None,
            description: Some(format!("Tool {i}").into()),
            input_schema: Arc::new(serde_json::Map::new()),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        };
        let executor: ToolExecutor = Arc::new(|_req| {
            Box::pin(async {
                Ok(rmcp::model::CallToolResult {
                    content: vec![],
                    is_error: Some(false),
                    meta: None,
                    structured_content: None,
                })
            })
        });
        registry.register(
            format!("tool_{i}"),
            RegisteredTool {
                metadata: tool,
                executor,
            },
        );
    }

    c.bench_function("tool lookup (100 tools)", |b| {
        b.iter(|| {
            let tool = registry.get("tool_50");
            black_box(tool.is_some())
        })
    });
}

criterion_group!(
    benches,
    bench_parse_todo_spec,
    bench_parse_petstore,
    bench_parse_prefect_large,
    bench_tool_registration,
    bench_tool_lookup,
);
criterion_main!(benches);
