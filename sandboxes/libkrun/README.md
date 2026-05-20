# libkrun / Firecracker Sandbox
#
# Runs yas-mcp tool calls in Firecracker microVMs via libkrun.
# libkrun provides the VM orchestration layer (TEE-compatible on AMD SEV).
#
# Install:
#   git clone https://github.com/containers/libkrun && cd libkrun && make
#
# Usage:
#   libkrun --image ubuntu:24.04 -- yas-mcp --swagger-file api.yaml --mode http
