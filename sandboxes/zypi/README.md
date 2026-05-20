# Zypi / Firecracker Sandbox
#
# Runs each MCP tool call in an ephemeral Firecracker microVM.
# This is the strongest isolation — each API call gets a fresh VM.
#
# Architecture:
#   yas-mcp (host) → zypi_exec → Firecracker VM → upstream API
#
# Zypi endpoint: http://zypi:4000
#
# Usage:
#   yas-mcp --swagger-file api.yaml --endpoint http://zypi:4000 --mode http
#
# The OpenAPI spec's `servers[0].url` should point to zypi,
# and zypi routes exec requests through Firecracker VMs.

# Zypi sandbox flags (set via adjustments.yaml or CLI)
#
# Per-tool sandbox mode:
#
# adjustments.yaml:
#   sandbox:
#     default: zypi
#     image: ubuntu:24.04
#     timeout: 30
#     per_route:
#       - path: /admin
#         methods: [DELETE]
#         sandbox: zypi
#         image: alpine:3.19    # smaller image for fast boot
#       - path: /health
#         methods: [GET]
#         sandbox: none         # no sandbox for health checks
#
# The yas-mcp requester wraps each HTTP call through
# POST http://zypi:4000/exec with the actual API call
# as the command inside the Firecracker VM.
