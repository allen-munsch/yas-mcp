#!/usr/bin/env python3
"""
yas-mcp Personal Agent — your local AI assistant.

Usage:
    python3 scripts/personal-agent.py                    # dashboard
    python3 scripts/personal-agent.py disk_usage         # run skill
    python3 scripts/personal-agent.py -c "ls -la"       # raw command
    python3 scripts/personal-agent.py -c "free -h"       # raw command

Requires the sandbox agent running: python3 scripts/sandbox-agent.py &
"""

import json
import os
import sys
import textwrap
import time
import uuid
from datetime import datetime
from pathlib import Path
from urllib.request import Request, urlopen

SANDBOX_URL = os.environ.get("SANDBOX_URL", "http://localhost:9002")
HISTORY_FILE = Path.home() / ".yas-mcp" / "agent_history.json"

SKILLS = {
    "list_files":    {"description": "List files",          "template": "shell", "params": lambda a: {"command": f"ls -la {a.get('path', '.')}"}},
    "find_files":    {"description": "Find files by name",  "template": "shell", "params": lambda a: {"command": f"find {a.get('path', '.')} -name '{a.get('pattern', '*')}' -type f 2>/dev/null"}},
    "disk_usage":    {"description": "Check disk usage",    "template": "shell", "params": lambda a: {"command": "df -h /"}},
    "memory_info":   {"description": "Show memory info",    "template": "shell", "params": lambda a: {"command": "free -h"}},
    "process_list":  {"description": "List top processes",  "template": "shell", "params": lambda a: {"command": "ps aux --sort=-%mem | head -20"}},
    "network_check": {"description": "Check connectivity",  "template": "shell", "params": lambda a: {"command": "ping -c 3 -W 2 8.8.8.8 2>&1 || echo offline"}},
    "system_info":   {"description": "System information",  "template": "shell", "params": lambda a: {"command": "uname -a && cat /etc/os-release 2>/dev/null | head -3"}},
    "fetch_url":     {"description": "Fetch a URL",         "template": "http_get", "params": lambda a: {"url": a.get("url", "http://example.com")}},
    "git_status":    {"description": "Git repo status",     "template": "shell", "params": lambda a: {"command": f"cd {a.get('repo', '.')} && git status --short 2>&1"}},
    "weather":       {"description": "City weather",       "template": "weather", "params": lambda a: {"city": a.get("city", "London")}},
    "calculate":     {"description": "Calculate",           "template": "calculator", "params": lambda a: {"expression": a.get("expression", "2+2")}},
}


def submit(skill, params=None):
    task_id = str(uuid.uuid4())[:8]
    payload = {"id": task_id, "sessionId": "cli", "message": {"role": "user", "parts": [{"type": "data", "data": {"skill": skill, "parameters": params or {}}}]}}
    try:
        req = Request(f"{SANDBOX_URL}/a2a/tasks/send", data=json.dumps(payload).encode(), headers={"Content-Type": "application/json"})
        with urlopen(req, timeout=60) as r:
            return json.loads(r.read())
    except Exception as e:
        return {"error": str(e), "id": task_id}


def load_hist(): return json.loads(HISTORY_FILE.read_text()) if HISTORY_FILE.exists() else []
def save_hist(h):
    HISTORY_FILE.parent.mkdir(parents=True, exist_ok=True)
    HISTORY_FILE.write_text(json.dumps(h, indent=2))


def main():
    args = sys.argv[1:]

    # Dashboard (no args)
    if not args:
        hist = load_hist()
        done = sum(1 for e in hist if e.get("status") == "completed")
        fail = sum(1 for e in hist if e.get("status") == "failed")
        print()
        print("  ╔══════════════════════════════════════════╗")
        print("  ║  ☀️  yas-mcp Personal Agent                ║")
        print("  ╚══════════════════════════════════════════╝")
        print(f"  Tasks: {len(hist)} ({done} done, {fail} failed)")
        if hist:
            print()
            for e in hist[-5:]:
                icon = "✅" if e.get("status") == "completed" else "❌"
                print(f"  {icon} {e['timestamp'][11:19]}  {e.get('skill','?')}")
        print()
        print("  Skills:")
        for name, info in SKILLS.items():
            print(f"    {name:<18} {info['description']}")
        print()
        print("  Try: python3 scripts/personal-agent.py disk_usage")
        print("       python3 scripts/personal-agent.py -c 'df -h'")
        return

    # Raw command mode: -c "command"
    if args[0] == "-c" and len(args) > 1:
        cmd = " ".join(args[1:])
        print(f"  🏗️  Running: {cmd}")
        result = submit("shell", {"command": cmd})
        skill_name = "shell"
    # Help
    elif args[0] in ("-h", "--help", "help"):
        print("Usage: python3 scripts/personal-agent.py [skill] [-c command]")
        print("Skills:", ", ".join(SKILLS.keys()))
        return
    # Named skill
    elif args[0] in SKILLS:
        skill_name = args[0]
        info = SKILLS[skill_name]
        print(f"  🏗️  {info['description']}...")
        result = submit(info["template"], info["params"]({}))
    else:
        print(f"  ❌ Unknown: {args[0]}")
        print(f"  Try: {', '.join(list(SKILLS.keys())[:5])}")
        return

    # Save history
    hist = load_hist()
    hist.append({"timestamp": datetime.now().isoformat(), "skill": skill_name, "status": result.get("status", {}).get("state", "unknown"), "result": result})
    save_hist(hist)

    # Show result
    if result.get("status", {}).get("state") == "completed":
        for art in result.get("artifacts", []):
            data = art.get("parts", [{}])[0].get("data", {})
            stdout = data.get("stdout", "")
            if stdout:
                print()
                print(textwrap.indent(stdout.strip(), "  "))
            exit_code = data.get("exit_code", "?")
            print(f"  ✅ Done (exit={exit_code})")
    else:
        print(f"  ❌ {result.get('error', 'failed')}")


if __name__ == "__main__":
    main()
