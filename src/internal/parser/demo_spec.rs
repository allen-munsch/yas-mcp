//! Built-in Demo Spec
//!
//! A small OpenAPI 3.0 spec bundled into the binary.
//! Used by `--demo` mode so users can try yas-mcp without any files.

pub const DEMO_SPEC: &str = r##"openapi: "3.0.3"
info:
  title: "yas-mcp Demo API"
  description: "Built-in demo API — try yas-mcp without any setup"
  version: "1.0.0"
servers:
  - url: "http://localhost:3000"
    description: "Demo server"
paths:
  /health:
    get:
      operationId: get_health
      summary: "Health check"
      description: "Returns server health status"
      responses:
        "200":
          description: "OK"
          content:
            application/json:
              schema:
                type: object
                properties:
                  status:
                    type: string
                    example: "healthy"
  /projects:
    get:
      operationId: list_projects
      summary: "List projects"
      description: "Returns a paginated list of projects with optional filtering"
      parameters:
        - name: page
          in: query
          schema:
            type: integer
            minimum: 1
            default: 1
        - name: status
          in: query
          schema:
            type: string
            enum: [active, archived, all]
      responses:
        "200":
          description: "List of projects"
          content:
            application/json:
              schema:
                type: object
                properties:
                  data:
                    type: array
                    items:
                      $ref: "#/components/schemas/Project"
                  total:
                    type: integer
                    example: 42
    post:
      operationId: create_project
      summary: "Create a project"
      description: "Creates a new project with the given details"
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name:
                  type: string
                  example: "My Project"
                description:
                  type: string
                  example: "A new project"
                color:
                  type: string
                  example: "#3B82F6"
      responses:
        "201":
          description: "Project created"
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Project"
  /projects/{id}:
    get:
      operationId: get_project
      summary: "Get a project"
      description: "Returns a single project by ID"
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
            format: uuid
      responses:
        "200":
          description: "Project details"
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Project"
    delete:
      operationId: delete_project
      summary: "Delete a project"
      description: "Deletes a project by ID"
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
            format: uuid
      responses:
        "204":
          description: "Deleted"
  /users/me:
    get:
      operationId: get_current_user
      summary: "Get current user"
      description: "Returns the authenticated user's profile"
      responses:
        "200":
          description: "User profile"
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/User"
components:
  schemas:
    Project:
      type: object
      properties:
        id:
          type: string
          format: uuid
          example: "550e8400-e29b-41d4-a716-446655440000"
        name:
          type: string
          example: "My Project"
        description:
          type: string
          example: "A sample project"
        color:
          type: string
          example: "#3B82F6"
        status:
          type: string
          enum: [active, archived]
          example: active
        created_at:
          type: string
          format: date-time
          example: "2025-01-15T10:30:00Z"
    User:
      type: object
      properties:
        id:
          type: string
          format: uuid
          example: "660e8400-e29b-41d4-a716-446655440001"
        name:
          type: string
          example: "Jane Developer"
        email:
          type: string
          format: email
          example: "jane@example.com"
        avatar:
          type: string
          format: uri
          example: "https://example.com/avatar.jpg"
"##;
