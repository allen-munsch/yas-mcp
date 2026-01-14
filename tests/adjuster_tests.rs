use yas_mcp::internal::parser::Adjuster;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper to create a temp file with YAML content
    fn create_temp_yaml(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write temp file");
        file
    }

    // ==================== new() tests ====================

    #[test]
    fn test_new_creates_empty_adjuster() {
        let adjuster = Adjuster::new();

        assert_eq!(adjuster.get_routes_count(), 0);
        assert!(adjuster.adjustments.descriptions.is_empty());
        assert!(adjuster.adjustments.routes.is_empty());
    }

    #[test]
    fn test_default_creates_empty_adjuster() {
        let adjuster = Adjuster::default();

        assert_eq!(adjuster.get_routes_count(), 0);
    }

    // ==================== load() tests ====================

    #[test]
    fn test_load_empty_path_returns_ok() {
        let mut adjuster = Adjuster::new();
        let result = adjuster.load("");

        assert!(result.is_ok());
        assert_eq!(adjuster.get_routes_count(), 0);
    }

    #[test]
    fn test_load_nonexistent_file_returns_ok() {
        let mut adjuster = Adjuster::new();
        let result = adjuster.load("/nonexistent/path/adjustments.yaml");

        // Should return Ok (matching Go behavior - file not found is not an error)
        assert!(result.is_ok());
        assert_eq!(adjuster.get_routes_count(), 0);
    }

    #[test]
    fn test_load_valid_routes_yaml() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
  - path: /projects
    methods: [GET, POST, DELETE]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();

        let result = adjuster.load(temp_file.path().to_str().unwrap());

        assert!(result.is_ok());
        assert_eq!(adjuster.get_routes_count(), 2);
    }

    #[test]
    fn test_load_valid_descriptions_yaml() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "Fetch all users from the system"
      - method: POST
        new_description: "Create a new user"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();

        let result = adjuster.load(temp_file.path().to_str().unwrap());

        assert!(result.is_ok());
        assert_eq!(adjuster.adjustments.descriptions.len(), 1);
        assert_eq!(adjuster.adjustments.descriptions[0].updates.len(), 2);
    }

    #[test]
    fn test_load_combined_routes_and_descriptions() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
  - path: /projects
    methods: [GET]

descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "List all users"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();

        let result = adjuster.load(temp_file.path().to_str().unwrap());

        assert!(result.is_ok());
        assert_eq!(adjuster.get_routes_count(), 2);
        assert_eq!(adjuster.adjustments.descriptions.len(), 1);
    }

    #[test]
    fn test_load_invalid_yaml_returns_error() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST
  invalid yaml here
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();

        let result = adjuster.load(temp_file.path().to_str().unwrap());

        assert!(result.is_err());
    }

    #[test]
    fn test_load_empty_yaml_file() {
        let yaml_content = "";
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();

        let result = adjuster.load(temp_file.path().to_str().unwrap());

        // Empty YAML should parse to default/empty struct
        assert!(result.is_ok());
        assert_eq!(adjuster.get_routes_count(), 0);
    }

    // ==================== exists_in_mcp() tests ====================

    #[test]
    fn test_exists_in_mcp_no_routes_allows_all() {
        let adjuster = Adjuster::new();

        // When no routes are configured, all routes should be allowed
        assert!(adjuster.exists_in_mcp("/users", "GET"));
        assert!(adjuster.exists_in_mcp("/projects", "POST"));
        assert!(adjuster.exists_in_mcp("/anything", "DELETE"));
    }

    #[test]
    fn test_exists_in_mcp_exact_match() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        assert!(adjuster.exists_in_mcp("/users", "GET"));
        assert!(adjuster.exists_in_mcp("/users", "POST"));
    }

    #[test]
    fn test_exists_in_mcp_method_not_allowed() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // DELETE is not in the allowed methods
        assert!(!adjuster.exists_in_mcp("/users", "DELETE"));
        assert!(!adjuster.exists_in_mcp("/users", "PUT"));
        assert!(!adjuster.exists_in_mcp("/users", "PATCH"));
    }

    #[test]
    fn test_exists_in_mcp_route_not_found() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // Route doesn't exist in adjustments
        assert!(!adjuster.exists_in_mcp("/projects", "GET"));
        assert!(!adjuster.exists_in_mcp("/unknown", "POST"));
    }

    #[test]
    fn test_exists_in_mcp_case_insensitive_method() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, post]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // Method comparison should be case-insensitive
        assert!(adjuster.exists_in_mcp("/users", "GET"));
        assert!(adjuster.exists_in_mcp("/users", "get"));
        assert!(adjuster.exists_in_mcp("/users", "Get"));
        assert!(adjuster.exists_in_mcp("/users", "POST"));
        assert!(adjuster.exists_in_mcp("/users", "post"));
        assert!(adjuster.exists_in_mcp("/users", "Post"));
    }

    #[test]
    fn test_exists_in_mcp_trailing_slash_normalization() {
        let yaml_content = r#"
routes:
  - path: /users/
    methods: [GET]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // Should match with or without trailing slash
        assert!(adjuster.exists_in_mcp("/users", "GET"));
        assert!(adjuster.exists_in_mcp("/users/", "GET"));
    }

    #[test]
    fn test_exists_in_mcp_multiple_routes() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET, POST]
  - path: /projects
    methods: [GET, DELETE]
  - path: /tasks
    methods: [GET, POST, PUT, DELETE]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // Users
        assert!(adjuster.exists_in_mcp("/users", "GET"));
        assert!(adjuster.exists_in_mcp("/users", "POST"));
        assert!(!adjuster.exists_in_mcp("/users", "DELETE"));

        // Projects
        assert!(adjuster.exists_in_mcp("/projects", "GET"));
        assert!(adjuster.exists_in_mcp("/projects", "DELETE"));
        assert!(!adjuster.exists_in_mcp("/projects", "POST"));

        // Tasks
        assert!(adjuster.exists_in_mcp("/tasks", "GET"));
        assert!(adjuster.exists_in_mcp("/tasks", "POST"));
        assert!(adjuster.exists_in_mcp("/tasks", "PUT"));
        assert!(adjuster.exists_in_mcp("/tasks", "DELETE"));
        assert!(!adjuster.exists_in_mcp("/tasks", "PATCH"));
    }

    #[test]
    fn test_exists_in_mcp_with_path_parameters() {
        let yaml_content = r#"
routes:
  - path: /users/{id}
    methods: [GET, PUT, DELETE]
  - path: /projects/{project_id}/tasks
    methods: [GET, POST]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        assert!(adjuster.exists_in_mcp("/users/{id}", "GET"));
        assert!(adjuster.exists_in_mcp("/users/{id}", "PUT"));
        assert!(adjuster.exists_in_mcp("/projects/{project_id}/tasks", "GET"));
        assert!(adjuster.exists_in_mcp("/projects/{project_id}/tasks", "POST"));
    }

    // ==================== get_description() tests ====================

    #[test]
    fn test_get_description_no_adjustments_returns_original() {
        let adjuster = Adjuster::new();
        let original = "Original description";

        let result = adjuster.get_description("/users", "GET", original);

        assert_eq!(result, original);
    }

    #[test]
    fn test_get_description_with_override() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "Fetch all users from the database"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        let result = adjuster.get_description("/users", "GET", "Original description");

        assert_eq!(result, "Fetch all users from the database");
    }

    #[test]
    fn test_get_description_no_matching_route() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "Fetch all users"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        let original = "Original description";
        let result = adjuster.get_description("/projects", "GET", original);

        assert_eq!(result, original);
    }

    #[test]
    fn test_get_description_no_matching_method() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "Fetch all users"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        let original = "Original description";
        let result = adjuster.get_description("/users", "POST", original);

        assert_eq!(result, original);
    }

    #[test]
    fn test_get_description_multiple_methods() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "List all users"
      - method: POST
        new_description: "Create a new user"
      - method: DELETE
        new_description: "Delete a user"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        assert_eq!(
            adjuster.get_description("/users", "GET", "original"),
            "List all users"
        );
        assert_eq!(
            adjuster.get_description("/users", "POST", "original"),
            "Create a new user"
        );
        assert_eq!(
            adjuster.get_description("/users", "DELETE", "original"),
            "Delete a user"
        );
        // PUT not in adjustments, should return original
        assert_eq!(
            adjuster.get_description("/users", "PUT", "original"),
            "original"
        );
    }

    #[test]
    fn test_get_description_multiple_routes() {
        let yaml_content = r#"
descriptions:
  - path: /users
    updates:
      - method: GET
        new_description: "Get users"
  - path: /projects
    updates:
      - method: GET
        new_description: "Get projects"
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        assert_eq!(
            adjuster.get_description("/users", "GET", "original"),
            "Get users"
        );
        assert_eq!(
            adjuster.get_description("/projects", "GET", "original"),
            "Get projects"
        );
    }

    // ==================== get_routes_count() tests ====================

    #[test]
    fn test_get_routes_count_empty() {
        let adjuster = Adjuster::new();
        assert_eq!(adjuster.get_routes_count(), 0);
    }

    #[test]
    fn test_get_routes_count_with_routes() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: [GET]
  - path: /projects
    methods: [GET, POST]
  - path: /tasks
    methods: [GET, POST, PUT]
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        assert_eq!(adjuster.get_routes_count(), 3);
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_empty_methods_list() {
        let yaml_content = r#"
routes:
  - path: /users
    methods: []
"#;
        let temp_file = create_temp_yaml(yaml_content);
        let mut adjuster = Adjuster::new();
        adjuster.load(temp_file.path().to_str().unwrap()).unwrap();

        // Route exists but no methods allowed
        assert!(!adjuster.exists_in_mcp("/users", "GET"));
        assert!(!adjuster.exists_in_mcp("/users", "POST"));
    }

    #[test]
    fn test_reload_overwrites_previous() {
        let yaml_content1 = r#"
routes:
  - path: /users
    methods: [GET]
"#;
        let yaml_content2 = r#"
routes:
  - path: /projects
    methods: [POST]
  - path: /tasks
    methods: [DELETE]
"#;
        let temp_file1 = create_temp_yaml(yaml_content1);
        let temp_file2 = create_temp_yaml(yaml_content2);
        let mut adjuster = Adjuster::new();

        adjuster.load(temp_file1.path().to_str().unwrap()).unwrap();
        assert_eq!(adjuster.get_routes_count(), 1);
        assert!(adjuster.exists_in_mcp("/users", "GET"));

        // Reload with new file
        adjuster.load(temp_file2.path().to_str().unwrap()).unwrap();
        assert_eq!(adjuster.get_routes_count(), 2);
        assert!(!adjuster.exists_in_mcp("/users", "GET")); // Old route gone
        assert!(adjuster.exists_in_mcp("/projects", "POST"));
        assert!(adjuster.exists_in_mcp("/tasks", "DELETE"));
    }
}