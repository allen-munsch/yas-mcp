#!/usr/bin/env python3
"""Sandbox A2A Agent — executes delegated tasks in isolated containers.

This agent receives A2A tasks from yas-mcp and runs them inside
sandboxed environments. Each tool call gets its own container.

Supported backends:
  docker    — Docker container with network isolation (default, works everywhere)
  libkrun   — Firecracker microVM via libkrun (stronger isolation, needs KVM)

Architecture:
  yas-mcp → A2A tasks/send → Sandbox Agent → Docker container → API call → result

Usage:
    pip install docker              # for Docker backend
    python3 scripts/sandbox-agent.py
    python3 scripts/sandbox-agent.py --backend libkrun  # experimental
"""

import json
import os
import subprocess
import sys
import tempfile
import time
import uuid
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs

BACKEND = "docker"
SANDBOX_IMAGE = os.environ.get("SANDBOX_IMAGE", "alpine:3.19")
NETWORK_MODE = os.environ.get("SANDBOX_NETWORK", "none")  # none = isolated

# ── Tool Registry ──────────────────────────────────────────────────────────

TOOLS = {
    "shell": {
        "description": "Execute a shell command in a sandboxed container",
        "handler": "run_sandboxed_command",
    },
    "http_get": {
        "description": "Make an HTTP GET request from a sandboxed container",
        "handler": "run_sandboxed_http",
    },
    "python": {
        "description": "Run a Python script in a sandboxed container",
        "handler": "run_sandboxed_python",
    },
}

# ── Task Store ─────────────────────────────────────────────────────────────

tasks = {}


def run_sandboxed_command(params):
    """Execute a shell command in an isolated Docker container."""
    cmd = params.get("command", "echo hello")
    image = params.get("image", SANDBOX_IMAGE)
    timeout = params.get("timeout", 30)

    result = subprocess.run(
        [
            "docker", "run", "--rm",
            "--network", NETWORK_MODE,
            "--memory", "64m",
            "--cpus", "0.5",
            "--pids-limit", "50",
            "--read-only",
            "--tmpfs", "/tmp:size=32m,noexec",
            "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges:true",
            image,
            "sh", "-c", cmd,
        ],
        capture_output=True,
        text=True,
        timeout=int(timeout),
    )
    return {
        "exit_code": result.returncode,
        "stdout": result.stdout[:2000],
        "stderr": result.stderr[:500],
        "sandbox": BACKEND,
        "image": image,
    }


def run_sandboxed_http(params):
    """Make an HTTP GET request from inside a sandboxed container."""
    url = params.get("url", "http://example.com")
    cmd = f"wget -q -O - --timeout=10 '{url}' 2>/dev/null || echo 'HTTP_REQUEST_FAILED'"
    return run_sandboxed_command({"command": cmd, **params})


def run_sandboxed_python(params):
    """Run a Python script in a sandboxed container."""
    script = params.get("script", "print('hello from sandbox')")
    # Write script to temp file, mount read-only into container
    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(script)
        script_path = f.name

    result = subprocess.run(
        [
            "docker", "run", "--rm",
            "--network", "none",
            "--memory", "64m",
            "--cpus", "0.5",
            "--pids-limit", "50",
            "--read-only",
            "--tmpfs", "/tmp:size=32m",
            "--cap-drop", "ALL",
            "-v", f"{script_path}:/script.py:ro",
            "python:3.12-alpine",
            "python", "/script.py",
        ],
        capture_output=True,
        text=True,
        timeout=15,
    )
    os.unlink(script_path)
    return {
        "exit_code": result.returncode,
        "stdout": result.stdout[:2000],
        "stderr": result.stderr[:500],
        "sandbox": BACKEND,
    }


def run_libkrun_command(params):
    """Execute a command in a Firecracker microVM via libkrun."""
    cmd = params.get("command", "echo hello")
    image = params.get("image", "alpine:latest")

    result = subprocess.run(
        [
            "libkrun", "run",
            "--image", image,
            "--memory", "128",
            "--cpus", "1",
            "--", "sh", "-c", cmd,
        ],
        capture_output=True,
        text=True,
        timeout=30,
    )
    return {
        "exit_code": result.returncode,
        "stdout": result.stdout[:2000],
        "stderr": result.stderr[:500],
        "sandbox": "libkrun",
        "image": image,
    }


# ── HTTP Handler ───────────────────────────────────────────────────────────


class SandboxAgentHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        print(f"  [sandbox] {args[0]}")

    def _send_json(self, data, status=200):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlparse(self.path).path

        if path == "/.well-known/agent-card.json":
            self._send_json({
                "name": "Sandbox Agent",
                "description": f"Executes tasks in isolated {BACKEND} containers",
                "url": "http://localhost:9002",
                "version": "0.1.0",
                "capabilities": {"streaming": False},
                "skills": [
                    {"id": name, "name": name, "description": info["description"],
                     "tags": [name, "sandbox", BACKEND],
                     "examples": [f"Run {name} in sandbox"],
                     "inputModes": ["application/json"], "outputModes": ["application/json"]}
                    for name, info in TOOLS.items()
                ],
                "defaultInputModes": ["application/json"],
                "defaultOutputModes": ["application/json"],
            })
            return

        if path == "/a2a/tasks/get":
            params = parse_qs(urlparse(self.path).query)
            task_id = params.get("id", [None])[0]
            if task_id and task_id in tasks:
                self._send_json(tasks[task_id])
            else:
                self._send_json({"error": "Task not found"}, 404)
            return

        if path == "/health":
            self._send_json({"status": "ok", "backend": BACKEND})
            return

        self._send_json({"error": "Not found"}, 404)

    def do_POST(self):
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length)) if length > 0 else {}

        if path == "/a2a/tasks/send":
            task_id = body.get("id", str(uuid.uuid4()))
            session_id = body.get("sessionId", "default")
            message = body.get("message", {})

            skill = None
            params = {}
            for part in message.get("parts", []):
                if part.get("type") == "data":
                    data = part.get("data", {})
                    skill = data.get("skill", skill)
                    params.update(data.get("parameters", {}))
                elif part.get("type") == "text":
                    if part.get("text", "") in TOOLS:
                        skill = part.get("text", "")

            if skill and skill in TOOLS:
                handler_name = TOOLS[skill]["handler"]
                handler = globals().get(handler_name)
                if handler:
                    try:
                        print(f"  [sandbox] 🏗️  Executing '{skill}' in {BACKEND} sandbox...")
                        result = handler(params)
                        status = "completed"
                        artifact = {
                            "artifactId": str(uuid.uuid4()),
                            "name": f"{skill}_result",
                            "parts": [{"type": "data", "data": result}],
                        }
                        print(f"  [sandbox] ✅ '{skill}' completed (exit={result.get('exit_code', '?')})")
                    except Exception as e:
                        result = {"error": str(e)}
                        status = "failed"
                        artifact = None
                        print(f"  [sandbox] ❌ '{skill}' failed: {e}")
                else:
                    result = {"error": f"Handler {handler_name} not found"}
                    status = "failed"
                    artifact = None
            else:
                result = {"error": f"Unknown skill: {skill}"}
                status = "failed"
                artifact = None

            task = {
                "id": task_id, "sessionId": session_id,
                "contextId": str(uuid.uuid4()),
                "status": {"state": status, "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ")},
                "artifacts": [artifact] if artifact else [],
            }
            tasks[task_id] = task
            self._send_json(task)
            return

        self._send_json({"error": "Not found"}, 404)


if __name__ == "__main__":
    import argparse
    p = argparse.ArgumentParser(description="Sandbox A2A Agent")
    p.add_argument("--backend", choices=["docker", "libkrun"], default="docker",
                   help="Sandbox backend (default: docker)")
    p.add_argument("--port", type=int, default=9002, help="Listen port")
    p.add_argument("--image", default="alpine:3.19", help="Container image")
    p.add_argument("--network", default="none", help="Container network mode")
    args = p.parse_args()

    BACKEND = args.backend
    SANDBOX_IMAGE = args.image
    NETWORK_MODE = args.network

    # Check Docker availability
    if BACKEND == "docker":
        try:
            subprocess.run(["docker", "version"], capture_output=True, check=True)
        except (subprocess.CalledProcessError, FileNotFoundError):
            print("❌ Docker not available. Install Docker or use --backend libkrun")
            sys.exit(1)

    server = HTTPServer(("0.0.0.0", args.port), SandboxAgentHandler)
    print(f"  🏗️  Sandbox Agent running on http://localhost:{args.port}")
    print(f"  Backend: {BACKEND} | Image: {SANDBOX_IMAGE} | Network: {NETWORK_MODE}")
    print(f"  Tools: {', '.join(TOOLS.keys())}")
    print(f"  Agent Card: http://localhost:{args.port}/.well-known/agent-card.json")
    print()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n  Sandbox Agent shutting down")
        server.shutdown()
