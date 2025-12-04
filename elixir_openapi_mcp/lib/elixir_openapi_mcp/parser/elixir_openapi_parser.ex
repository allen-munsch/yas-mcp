defmodule ElixirOpenapiMcp.Parser.ElixirOpenapiParser do
  @moduledoc """
  Parses an OpenAPI 3.0 specification file.
  """

  @behaviour ElixirOpenapiMcp.Parser.Parser

  alias ElixirOpenapiMcp.Parser.Types.{McpTool, RouteTool}
  alias Path

  defstruct [:spec, :adjustments]

  @impl ElixirOpenapiMcp.Parser.Parser
  def init(swagger_file, adjustments_file) do
    with {:ok, spec} <- load_spec(swagger_file),
         {:ok, adjustments} <- load_adjustments(adjustments_file) do
      {:ok, %__MODULE__{spec: spec, adjustments: adjustments}}
    end
  end

  @impl ElixirOpenapiMcp.Parser.Parser
  def get_route_tools(state) do
    spec = state.spec
    adjustments = state.adjustments

    paths = Map.get(spec, "paths", %{})

    for {path, methods} <- paths,
        {method, operation} <- methods,
        reduce: [] do
      acc ->
        # Filter routes based on adjustments
        if ElixirOpenapiMcp.Parser.Adjuster.exists_in_mcp?(path, method, adjustments) do
          tool_name = normalize_tool_name(path, method)
          description =
            ElixirOpenapiMcp.Parser.Adjuster.get_description(
              path,
              method,
              Map.get(operation, "summary"),
              adjustments
            )

          input_schema = extract_input_schema(operation)
          output_schema = extract_output_schema(operation)

          mcp_tool = %McpTool{
            name: tool_name,
            description: description,
            input_schema: input_schema,
            output_schema: output_schema,
            annotations: %{} # TODO: Extract annotations from OpenAPI spec
          }

          route_config = %ElixirOpenapiMcp.Requester.Types.RouteConfig{
            path: path,
            method: method,
            description: description,
            headers: %{}, # TODO: Extract headers
            parameters: extract_parameters(operation),
            method_config: %ElixirOpenapiMcp.Requester.Types.MethodConfig{} # TODO: Fill this
          }

          [
            %RouteTool{
              route_config: route_config,
              tool: mcp_tool
            }
            | acc
          ]
        else
          acc
        end
    end
  end

  defp normalize_tool_name(path, method) do
    path
    |> String.replace(~r"\{(\w+\})", "_")
    |> String.replace("/", "_")
    |> String.replace("-", "_")
    |> String.trim_leading("_")
    |> Kernel.<>(method)
    |> String.downcase()
  end

  defp extract_input_schema(operation) do
    # This is a placeholder. A more robust implementation would be needed here.
    # It should extract parameters (path, query, header, cookie) and requestBody
    %{type: :object, properties: %{}}
  end

  defp extract_output_schema(operation) do
    # This is a placeholder. A more robust implementation would be needed here.
    # It should extract responses
    %{type: :object, properties: %{}}
  end

  defp extract_parameters(operation) do
    # This is a placeholder. A more robust implementation would be needed here.
    Map.get(operation, "parameters", [])
  end

  defp load_spec(swagger_file) do
    # IO.puts("Debug: swagger_file received: #{inspect(swagger_file)}")
    # current_cwd = File.cwd!()
    # IO.puts("Debug: Current CWD: #{inspect(current_cwd)}")
    absolute_swagger_file = Path.expand(swagger_file, File.cwd!())
    # IO.puts("Debug: Expanded absolute_swagger_file: #{inspect(absolute_swagger_file)}")
    case File.read(absolute_swagger_file) do
      {:ok, body} ->
        # IO.puts("Debug: Successfully read file: #{absolute_swagger_file}")
        # The body will be a charlist, convert to string for Jason and Yamerl
        # body_string = to_string(body)
        cond do
          String.ends_with?(absolute_swagger_file, ".json") -> Jason.decode(body)
          String.ends_with?(absolute_swagger_file, ".yaml") or String.ends_with?(absolute_swagger_file, ".yml") -> Yaml.decode(body)
          true -> {:error, "Unsupported file type"}
        end
      {:error, reason} ->
        # IO.puts("Debug: Failed to read file: #{absolute_swagger_file} with reason: #{inspect(reason)}")
        {:error, "Failed to read swagger file: #{reason}"}
    end
  end

  defp load_adjustments(nil) do
    {:ok, %{}}
  end

  defp load_adjustments(adjustments_file) do
    absolute_adjustments_file = Path.expand(adjustments_file, File.cwd!())
    case File.read(absolute_adjustments_file) do
      {:ok, body} ->
        # The body will be a charlist, convert to string for Yamerl
        # body_string = to_string(body)
        case Yaml.decode(body) do
          {:ok, data} -> {:ok, data}
          {:error, reason} -> {:error, "Failed to decode YAML file: #{reason}"}
          _ -> {:error, "Failed to decode YAML file: Unknown error"}
        end
      {:error, reason} ->
        {:error, "Failed to read adjustments file: #{reason}"}
    end
  end
end
