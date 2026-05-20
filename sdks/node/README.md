# Using yas-mcp from Node.js

No npm package needed. Just copy `client.js` into your project — it's ~60 lines, zero dependencies, works everywhere.

## Quick Start

```bash
# Install the yas-mcp binary (one-liner)
curl -fsSL https://raw.githubusercontent.com/allen-munsch/yas-mcp/main/sdks/install.sh | sh

# Start the server
yas-mcp --swagger-file api.yaml --mock --mode http
```

## Usage

```js
// Copy client.js into your project, then:
const yas = require("./client");

async function main() {
  const serverUrl = "http://localhost:3000";

  // List tools
  const tools = await yas.listTools(serverUrl);
  console.log(`${tools.length} tools available`);

  // Call a tool
  const result = await yas.callTool(serverUrl, "listPets", { page: 1 });
  console.log(result);
}

main();
```

The `client.js` file uses only `fetch()` — works in Node 18+, Bun, Deno, and browsers.
