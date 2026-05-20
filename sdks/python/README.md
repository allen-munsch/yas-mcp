# Using yas-mcp from Python

No PyPI package needed. Just copy `client.py` into your project — it's ~80 lines, zero dependencies, stdlib only.

## Quick Start

```bash
# Install the yas-mcp binary (one-liner)
curl -fsSL https://raw.githubusercontent.com/allen-munsch/yas-mcp/main/sdks/install.sh | sh

# Start the server
yas-mcp --swagger-file api.yaml --mock --mode http
```

## Usage

```python
# Copy client.py into your project, then:
from client import Client

c = Client("http://localhost:3000")

# List tools
tools = c.list_tools()
print(f"{len(tools)} tools available")

# Call a tool
result = c.call_tool("listPets", page=1)
print(result)
```

The `client.py` file uses only `urllib` — no dependencies, works on Python 3.9+.
