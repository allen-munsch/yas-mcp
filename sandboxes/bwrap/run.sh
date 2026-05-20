# bubblewrap Sandbox
#
# Runs yas-mcp in a minimal container using Linux user namespaces.
# No root required, no Docker daemon, just bwrap.
#
# Install: apt install bubblewrap  (or your distro's package)
#
# Usage:
#   bash sandboxes/bwrap/run.sh

# Create a read-only root by bind-mounting what we need
bwrap \
  --tmpfs / \
  --ro-bind /usr /usr \
  --ro-bind /bin /bin \
  --ro-bind /lib /lib \
  --ro-bind /lib64 /lib64 \
  --ro-bind /etc/ssl /etc/ssl \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --ro-bind "$(pwd)/examples/todo-app/openapi.yaml" /config/openapi.yaml \
  --ro-bind "$(pwd)/config.yaml" /config/config.yaml \
  --bind "$(pwd)/logs" /logs \
  --unshare-all \
  --share-net \
  --die-with-parent \
  --setenv RUST_LOG info \
  --chdir /tmp \
  /usr/local/bin/yas-mcp \
  --config /config/config.yaml \
  --swagger-file /config/openapi.yaml \
  --mode http \
  --host 0.0.0.0 \
  --port 3000
