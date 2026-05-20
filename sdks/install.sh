#!/bin/sh
# yas-mcp — one-line installer for Linux and macOS
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/allen-munsch/yas-mcp/main/sdks/install.sh | sh
#
# Or:
#   wget -qO- https://raw.githubusercontent.com/allen-munsch/yas-mcp/main/sdks/install.sh | sh

set -e

VERSION="${YAS_MCP_VERSION:-0.1.0}"
INSTALL_DIR="${YAS_MCP_INSTALL_DIR:-/usr/local/bin}"
BINARY="yas-mcp"

# ── Detect platform ───────────────────────────────────────────────────────
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

case "$OS" in
    linux)   TARGET="${ARCH}-unknown-linux-musl" ;;
    darwin)  TARGET="${ARCH}-apple-darwin" ;;
    *)       echo "Unsupported OS: $OS"; exit 1 ;;
esac

URL="https://github.com/allen-munsch/yas-mcp/releases/download/v${VERSION}/yas-mcp-${TARGET}"

echo ""
echo "  ╔══════════════════════════════════════════════╗"
echo "  ║  ☀️  yas-mcp installer                        ║"
echo "  ╚══════════════════════════════════════════════╝"
echo ""
echo "  Platform: ${OS}/${ARCH}"
echo "  Version:  v${VERSION}"
echo "  Install:  ${INSTALL_DIR}/${BINARY}"
echo ""

# ── Check if already installed ────────────────────────────────────────────
if command -v yas-mcp > /dev/null 2>&1; then
    CURRENT=$(yas-mcp --version 2>/dev/null | head -1 || echo "unknown")
    echo "  yas-mcp is already installed: ${CURRENT}"
    echo "  To reinstall, run: curl ... | sh"
    exit 0
fi

# ── Download ──────────────────────────────────────────────────────────────
echo "  Downloading..."
if command -v curl > /dev/null 2>&1; then
    curl -fsSL "$URL" -o "/tmp/${BINARY}"
elif command -v wget > /dev/null 2>&1; then
    wget -q "$URL" -O "/tmp/${BINARY}"
else
    echo "  ❌ Need curl or wget to download"
    exit 1
fi

# ── Install ───────────────────────────────────────────────────────────────
chmod +x "/tmp/${BINARY}"

if [ -w "$INSTALL_DIR" ]; then
    mv "/tmp/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
    sudo mv "/tmp/${BINARY}" "${INSTALL_DIR}/${BINARY}"
fi

echo "  ✅ yas-mcp installed to ${INSTALL_DIR}/${BINARY}"
echo ""
echo "  Try it:"
echo "    yas-mcp --help"
echo "    yas-mcp --swagger-file api.yaml --mode http"
echo ""
