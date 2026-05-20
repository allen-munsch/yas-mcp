"""yas-mcp HTTP Client — thin convenience wrapper.

Copy this file into your project. Zero dependencies, stdlib only.
Works on Python 3.9+.

Usage:
    from client import Client
    c = Client("http://localhost:3000")
    tools = c.list_tools()
    result = c.call_tool("listPets", page=1)
"""

import json
from typing import Any
from urllib.request import Request, urlopen
from urllib.error import URLError


class Client:
    def __init__(self, server_url: str):
        self.url = server_url.rstrip("/")

    def _mcp(self, method: str, params: dict | None = None) -> Any:
        payload = {
            "jsonrpc": "2.0",
            "id": "py-1",
            "method": method,
            "params": params or {},
        }
        req = Request(
            f"{self.url}/mcp",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"},
        )
        try:
            with urlopen(req) as resp:
                data = json.loads(resp.read())
        except URLError as e:
            raise ConnectionError(f"MCP request failed: {e}") from e

        if "error" in data:
            raise RuntimeError(
                f"MCP error {data['error']['code']}: {data['error']['message']}"
            )
        return data["result"]

    def initialize(self) -> dict:
        return self._mcp("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "yas-mcp-py", "version": "0.1.0"},
        })

    def list_tools(self) -> list[dict]:
        return self._mcp("tools/list")["tools"]

    def call_tool(self, name: str, **args) -> Any:
        return self._mcp("tools/call", {"name": name, "arguments": args})

    def ping(self) -> dict:
        return self._mcp("ping")

    def get_catalog(self) -> dict:
        req = Request(f"{self.url}/.well-known/ai-catalog.json")
        with urlopen(req) as resp:
            return json.loads(resp.read())

    def get_agent_card(self) -> dict:
        req = Request(f"{self.url}/.well-known/agent-card.json")
        with urlopen(req) as resp:
            return json.loads(resp.read())

    def send_task(self, task_id: str, session_id: str, skill: str, **params) -> dict:
        payload = {
            "id": task_id,
            "sessionId": session_id,
            "message": {
                "role": "user",
                "parts": [{"type": "data", "data": {"skill": skill, "parameters": params}}],
            },
        }
        req = Request(
            f"{self.url}/a2a/tasks/send",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"},
        )
        with urlopen(req) as resp:
            return json.loads(resp.read())
