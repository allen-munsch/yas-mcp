#!/usr/bin/env python3
"""Mini A2A Agent — receives delegated tasks and executes them.

This is a tiny agent that implements the A2A protocol.
yas-mcp can delegate tasks to it, and it executes them
using its own tool registry.

Usage:
    python3 scripts/mini-agent.py
    # Agent starts on http://localhost:9001
    # yas-mcp can delegate via POST http://localhost:9001/a2a/tasks/send
"""

import json
import time
import uuid
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs

# ── Mini Agent's Tool Registry ────────────────────────────────────────────

TOOLS = {
    "weather": {
        "description": "Get current weather for a city",
        "handler": lambda params: {
            "city": params.get("city", "unknown"),
            "temperature": 22,
            "conditions": "sunny",
            "humidity": 45,
        },
    },
    "calculator": {
        "description": "Perform a calculation",
        "handler": lambda params: {
            "expression": params.get("expression", "0"),
            "result": eval(str(params.get("expression", "0"))),
        },
    },
    "translate": {
        "description": "Translate text to another language",
        "handler": lambda params: {
            "text": params.get("text", ""),
            "from": params.get("from", "en"),
            "to": params.get("to", "es"),
            "translated": f"[{params.get('to', 'es')}] {params.get('text', '')}",
        },
    },
}

# ── Task Store ─────────────────────────────────────────────────────────────

tasks = {}


class AgentHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"  [agent] {args[0]}")

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
                "name": "Mini Agent",
                "description": "A tiny A2A agent with weather, calculator, and translation tools",
                "url": "http://localhost:9001",
                "version": "0.1.0",
                "capabilities": {"streaming": False, "pushNotifications": False},
                "skills": [
                    {"id": name, "name": name, "description": info["description"],
                     "tags": [name], "examples": [f"Use {name}"],
                     "inputModes": ["application/json"], "outputModes": ["application/json"]}
                    for name, info in TOOLS.items()
                ],
                "defaultInputModes": ["text", "application/json"],
                "defaultOutputModes": ["text", "application/json"],
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
            self._send_json({"status": "ok"})
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

            # Extract skill and parameters
            skill = None
            params = {}
            for part in message.get("parts", []):
                if part.get("type") == "data":
                    data = part.get("data", {})
                    skill = data.get("skill", skill)
                    params.update(data.get("parameters", {}))
                elif part.get("type") == "text":
                    # Try text as skill name
                    text = part.get("text", "")
                    if text in TOOLS:
                        skill = text

            # Execute the tool
            if skill and skill in TOOLS:
                try:
                    result = TOOLS[skill]["handler"](params)
                    status = "completed"
                    artifact = {
                        "artifactId": str(uuid.uuid4()),
                        "name": f"{skill}_result",
                        "parts": [{"type": "data", "data": result}],
                    }
                except Exception as e:
                    result = {"error": str(e)}
                    status = "failed"
                    artifact = None
            else:
                result = {"error": f"Unknown skill: {skill}"}
                status = "failed"
                artifact = None

            task = {
                "id": task_id,
                "sessionId": session_id,
                "contextId": str(uuid.uuid4()),
                "status": {"state": status, "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ")},
                "artifacts": [artifact] if artifact else [],
                "history": [
                    {"state": "submitted", "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ")},
                    {"state": status, "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ")},
                ],
            }
            tasks[task_id] = task
            self._send_json(task)
            return

        if path == "/a2a/tasks/cancel":
            task_id = body.get("id")
            if task_id and task_id in tasks:
                tasks[task_id]["status"]["state"] = "canceled"
                self._send_json(tasks[task_id])
            else:
                self._send_json({"error": "Task not found"}, 404)
            return

        self._send_json({"error": "Not found"}, 404)


if __name__ == "__main__":
    port = 9001
    server = HTTPServer(("0.0.0.0", port), AgentHandler)
    print(f"  🤖 Mini A2A Agent running on http://localhost:{port}")
    print(f"  Tools: {', '.join(TOOLS.keys())}")
    print(f"  Agent Card: http://localhost:{port}/.well-known/agent-card.json")
    print()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n  Agent shutting down")
        server.shutdown()
