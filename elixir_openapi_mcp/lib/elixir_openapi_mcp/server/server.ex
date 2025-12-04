  use Hermes.Server,
    name: "Elixir OpenAPI MCP Server",
    version: "0.1.0",
    capabilities: [:tools]

  alias ElixirOpenapiMcp.Config.Config
  alias ElixirOpenapiMcp.Parser.ElixirOpenapiParser
  alias ElixirOpenapiMcp.Server.ToolHandler

  @impl true
  def init(_client_info, frame, config) do
    route_tools = ElixirOpenapiMcp.Parser.ElixirOpenapiParser.get_route_tools(parser_state)

    frame =
      Enum.reduce(route_tools, frame, fn %{route_config: route_config, tool: mcp_tool}, acc_frame ->
        register_tool(acc_frame, mcp_tool.name,
          input_schema: mcp_tool.input_schema,
          annotations: mcp_tool.annotations,
          description: mcp_tool.description,
          handler: {ElixirOpenapiMcp.Server.ToolHandler, :handle_tool, [route_config, config]})
      end)

    {:ok, frame}
  end

  @impl true
  def handle_tool("dummy_tool", %{text: text}, frame) do
    Logger.info("Dummy tool called with text: #{text}")
    {:reply, "Echo: #{text}", frame}
  end

  defp start_server(%{mode: :stdio}) do
    # This will be handled by Hermes.Server
  end

  defp start_server(%{mode: :http, port: port}) do
    # This will be handled by Hermes.Server
  end

  defp start_server(%{mode: :sse, port: port}) do
    # This will be handled by Hermes.Server
  end
end
