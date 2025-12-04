defmodule ElixirOpenapiMcp.CLI do
  @moduledoc """
  Parses command-line arguments.
  """

  alias Optimus

  def main(args) do
    case Optimus.parse(args, cli_spec()) do
      {:ok, parsed_args} ->
        case parsed_args[:version] do
          true ->
            IO.puts("Elixir OpenAPI MCP 0.1.0")
            System.halt(0)

          _ ->
            case ElixirOpenapiMcp.Config.Loader.load(parsed_args) do
              {:ok, config} ->
                ElixirOpenapiMcp.start_app(config)

              {:error, reason} ->
                IO.puts("Error loading config: #{reason}")
                System.halt(1)
            end
        end

      {:error, reason} ->
        IO.puts("Error parsing arguments: #{reason}")
        System.halt(1)
    end
  end

  defp cli_spec do
    [
      port: [
        type: :integer,
        default: 3000,
        aliases: [:p],
        doc: "The port to listen on"
      ],
      host: [
        type: :string,
        default: "127.0.0.1",
        aliases: [:h],
        doc: "The host to bind to"
      ],
      mode: [
        type: :string,
        default: "stdio",
        values: ["stdio", "http", "sse"],
        aliases: [:m],
        doc: "The server mode (stdio, http, sse)"
      ],
      swagger_file: [
        type: :string,
        required: true,
        aliases: [:s],
        doc: "Path to the OpenAPI/Swagger specification file"
      ],
      adjustments_file: [
        type: :string,
        aliases: [:a],
        doc: "Path to the adjustments YAML file"
      ],
      config: [
        type: :string,
        aliases: [:c],
        doc: "Path to the application configuration file"
      ],
      endpoint: [
        type: :string,
        doc: "The base URL for the external API endpoint"
      ],
      version: [
        type: :boolean,
        default: false,
        doc: "Show version information"
      ]
    ]
  end
end
