// yas-mcp HTTP Client — thin convenience wrapper
//
// Usage:
//   const yas = require('yas-mcp/client');
//   const tools = await yas.listTools('http://localhost:3000');
//   const result = await yas.callTool('http://localhost:3000', 'listPets', { page: 1 });

async function mcpRequest(serverUrl, method, params = {}) {
  const res = await fetch(`${serverUrl}/mcp`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: Date.now().toString(),
      method,
      params,
    }),
  });
  const data = await res.json();
  if (data.error) throw new Error(`MCP error ${data.error.code}: ${data.error.message}`);
  return data.result;
}

/** Initialize connection, returns server info */
async function initialize(serverUrl) {
  return mcpRequest(serverUrl, "initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "yas-mcp-js", version: "0.1.0" },
  });
}

/** List all available tools */
async function listTools(serverUrl) {
  const result = await mcpRequest(serverUrl, "tools/list");
  return result.tools;
}

/** Call a specific tool by name */
async function callTool(serverUrl, toolName, args = {}) {
  return mcpRequest(serverUrl, "tools/call", { name: toolName, arguments: args });
}

/** Ping the server */
async function ping(serverUrl) {
  return mcpRequest(serverUrl, "ping");
}

/** Fetch the AI Catalog */
async function getCatalog(serverUrl) {
  const res = await fetch(`${serverUrl}/.well-known/ai-catalog.json`);
  return res.json();
}

/** Fetch the A2A Agent Card */
async function getAgentCard(serverUrl) {
  const res = await fetch(`${serverUrl}/.well-known/agent-card.json`);
  return res.json();
}

/** Send an A2A task */
async function sendTask(serverUrl, taskId, sessionId, skill, params = {}) {
  const res = await fetch(`${serverUrl}/a2a/tasks/send`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      id: taskId,
      sessionId,
      message: {
        role: "user",
        parts: [{ type: "data", data: { skill, parameters: params } }],
      },
    }),
  });
  return res.json();
}

module.exports = {
  initialize,
  listTools,
  callTool,
  ping,
  getCatalog,
  getAgentCard,
  sendTask,
};
