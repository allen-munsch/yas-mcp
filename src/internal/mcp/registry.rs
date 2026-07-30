use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::internal::server::tool::handler::ToolExecutor;
use rmcp::model::Tool;

pub struct RegisteredTool {
    pub metadata: Tool,
    pub executor: ToolExecutor,
}

/// Thread-safe tool registry that can be shared across transports
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<RegisteredTool>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }
    pub fn register(&self, name: String, tool: RegisteredTool) {
        self.tools.write().unwrap().insert(name, Arc::new(tool));
    }
    pub fn get(&self, name: &str) -> Option<Arc<RegisteredTool>> {
        self.tools.read().unwrap().get(name).cloned()
    }
    pub fn list_metadata(&self) -> Vec<Tool> {
        self.tools
            .read()
            .unwrap()
            .values()
            .map(|tool| tool.metadata.clone())
            .collect()
    }
    pub fn count(&self) -> usize {
        self.tools.read().unwrap().len()
    }

    /// Clear all registered tools (useful for testing)
    #[cfg(test)]
    pub fn clear(&self) {
        self.tools.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::server::tool::handler::ToolExecutor;
    use std::sync::Arc;

    fn make_test_tool(name: &'static str) -> RegisteredTool {
        let tool = rmcp::model::Tool::new(
            name,
            format!("Tool {name}"),
            Arc::new(serde_json::Map::new()),
        );

        let executor: ToolExecutor =
            Arc::new(|_req| Box::pin(async { Ok(rmcp::model::CallToolResult::success(vec![])) }));

        RegisteredTool {
            metadata: tool,
            executor,
        }
    }

    #[test]
    fn test_register_and_get() {
        let registry = ToolRegistry::new();
        let tool = make_test_tool("test1");

        registry.register("test1".into(), tool);

        let retrieved = registry.get("test1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().metadata.name, "test1");
    }

    #[test]
    fn test_get_nonexistent() {
        let registry = ToolRegistry::new();
        let result = registry.get("missing");
        assert!(result.is_none());
    }

    #[test]
    fn test_count() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 0);

        registry.register("a".into(), make_test_tool("a"));
        assert_eq!(registry.count(), 1);

        registry.register("b".into(), make_test_tool("b"));
        assert_eq!(registry.count(), 2);

        // Re-registering should overwrite, not add
        registry.register("a".into(), make_test_tool("a_v2"));
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn test_list_metadata() {
        let registry = ToolRegistry::new();
        registry.register("first".into(), make_test_tool("first"));
        registry.register("second".into(), make_test_tool("second"));

        let tools = registry.list_metadata();
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(names.contains(&"first"));
        assert!(names.contains(&"second"));
    }

    #[test]
    fn test_clear() {
        let registry = ToolRegistry::new();
        registry.register("a".into(), make_test_tool("a"));
        registry.register("b".into(), make_test_tool("b"));
        assert_eq!(registry.count(), 2);

        registry.clear();
        assert_eq!(registry.count(), 0);
        assert!(registry.get("a").is_none());
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;

        let registry = Arc::new(ToolRegistry::new());
        let r1 = registry.clone();
        let r2 = registry.clone();

        // Use static string literals stored as leaked strings for test purposes
        let t1 = thread::spawn(move || {
            for i in 0..50 {
                let name: &'static str = Box::leak(format!("tool_{i}").into_boxed_str());
                r1.register(name.to_string(), make_test_tool(name));
            }
        });

        let t2 = thread::spawn(move || {
            for i in 50..100 {
                let name: &'static str = Box::leak(format!("tool_{i}").into_boxed_str());
                r2.register(name.to_string(), make_test_tool(name));
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();

        assert_eq!(registry.count(), 100);
    }
}
