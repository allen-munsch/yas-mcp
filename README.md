# OpenAPI MCP Server

![fmt](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml/badge.svg?job=fmt)
![clippy](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml/badge.svg?job=clippy)
![build](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml/badge.svg?job=build)
![security](https://github.com/allen-munsch/yas-mcp/actions/workflows/ci.yml/badge.svg?job=security)


A Rust-based Model Context Protocol (MCP) server that automatically exposes OpenAPI/Swagger endpoints as MCP tools.

## Overview

This server bridges OpenAPI specifications with the Model Context Protocol, allowing AI assistants to interact with REST APIs through standardized MCP tools. It supports multiple operation modes and includes OAuth2 authentication capabilities.

## Features

- Parse OpenAPI 3.0 specifications (JSON/YAML)
- Generate MCP tools from API endpoints
- Multiple server modes: STDIO, HTTP, SSE
- Route filtering and description customization
- OAuth2 authentication support (GitHub, Google, Microsoft, Generic)
- Docker support with Keycloak integration

## Installation

### From Source

```bash
cargo build --release
```

The binary will be available at `target/release/yas-mcp`

### Using Docker

```bash
docker compose up
```

## Usage

### Basic Usage

```bash
yas-mcp --swagger-file path/to/openapi.yaml --mode stdio
```

### Configuration File

Create a `config.yaml`:

```yaml
server:
  mode: stdio
  host: 127.0.0.1
  port: 3000
  name: yas-mcp
  version: 0.1.0

logging:
  level: info
  format: compact
  color: true

endpoint:
  base_url: http://localhost:8080
  auth_type: none

swagger_file: examples/todo-app/openapi.yaml
```

Then run:

```bash
yas-mcp --config config.yaml
```

### Command Line Options

- `--mode`: Server mode (stdio, http, sse). Default: stdio
- `--swagger-file`: Path to OpenAPI specification (required)
- `--adjustments-file`: Path to adjustments file for filtering/customization
- `--config`: Path to configuration file
- `--endpoint`: API endpoint base URL
- `--host`: Server host for HTTP/SSE modes
- `--port`: Server port for HTTP/SSE modes

## Server Modes

### STDIO Mode

Primary MCP mode for direct integration with AI assistants:

```bash
yas-mcp --swagger-file api.yaml --mode stdio
```

### HTTP Mode

JSON-RPC over HTTP with session management:

```bash
yas-mcp --swagger-file api.yaml --mode http --port 3000
```

Endpoints:
- POST `/mcp` - Main JSON-RPC endpoint
- GET `/sse` - Server-Sent Events stream
- DELETE `/session` - Session cleanup
- GET `/health` - Health check

### SSE Mode

Server-Sent Events for streaming responses:

```bash
yas-mcp --swagger-file api.yaml --mode sse --port 3000
```

## Adjustments File

Filter routes and customize descriptions using YAML:

```yaml
routes:
  - path: /users/me
    methods:
      - GET
  - path: /todos
    methods:
      - GET
      - POST

descriptions:
  - path: /todos
    updates:
      - method: GET
        new_description: Retrieve all todo items with optional filtering
```

## OAuth2 Authentication

Configure OAuth2 in `config.yaml`:

```yaml
oauth:
  enabled: true
  provider: github
  client_id: your_client_id
  client_secret: your_client_secret
  scopes:
    - read:user
    - user:email
  redirect_uri: http://localhost:3000/oauth/callback
```

Supported providers: github, google, microsoft, generic

### Using Keycloak

Scripts are provided for local Keycloak testing:

```bash
# Start Keycloak
docker compose --profile auth up

# Setup Keycloak realm and client
./scripts/setup-keycloak.sh

# Generate OAuth configuration
./scripts/create-config.sh
```

## Development

### Running Tests

```bash
cargo test
```

### With Mock Server

```bash
# Start Prism mock server
docker compose up prism

# Run tests
cargo test -- --nocapture
```

### Project Structure

- `src/main.rs` - Application entry point
- `src/internal/server/` - Server implementations
- `src/internal/parser/` - OpenAPI parsing
- `src/internal/requester/` - HTTP client
- `src/internal/auth/` - OAuth2 implementation
- `src/internal/config/` - Configuration management

## Environment Variables

Configuration can be overridden with environment variables using the `OPENAPI_MCP_` prefix:

```bash
export OPENAPI_MCP_SERVER_PORT=3000
export OPENAPI_MCP_LOGGING_LEVEL=debug
export OPENAPI_MCP_ENDPOINT_BASE_URL=http://api.example.com
```

## Examples

See the `examples/` directory for sample configurations:

- `examples/todo-app/` - Todo API with adjustments
- Docker Compose configurations
- OAuth2 setup scripts

# A brief security consideration

If you have not read the openapi spec that you plan to use, then you may want to run it in a sandbox:

```
bwrap \
--tmpfs / \
--ro-bind /usr /usr \
--ro-bind /bin /bin \
--ro-bind /lib /lib \
--ro-bind /lib64 /lib64 \
--ro-bind /lib/x86_64-linux-gnu /lib/x86_64-linux-gnu \
--proc /proc \
--dev-bind /dev/null /dev/null \
--tmpfs /tmp \
--ro-bind ~/.cargo ~/.cargo \
--ro-bind ~/.rustup ~/.rustup \
--ro-bind "$(pwd)" /build \
--tmpfs /build/logs \
--tmpfs /build/target \
--ro-bind "$(pwd)/examples/todo-app/openapi.yaml" /config/openapi.yaml \
--ro-bind "$(pwd)/mcp-oauth-config.yaml" /config/mcp-oauth-config.yaml \
--setenv CC /usr/bin/x86_64-linux-gnu-gcc-11 \
--setenv CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER /usr/bin/x86_64-linux-gnu-gcc-11 \
--setenv PATH /usr/bin:/bin \
--chdir /build \
--unshare-all \
--share-net \
--die-with-parent \
cargo run \
--bin yas-mcp \
-- \
--config /config/mcp-oauth-config.yaml \
--swagger-file /config/openapi.yaml \
--mode http
```

Or, maybe in docker, or better yet don't run it at all on your machine.


## License

See LICENSE file for details.

## Contributing

Contributions are welcome. Please ensure tests pass and follow existing code style.
