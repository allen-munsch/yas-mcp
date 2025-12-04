defmodule ElixirOpenapiMcp.Config.Loader do
  @moduledoc """
  Loads the application configuration from a YAML file, environment variables, and CLI arguments.
  """

  alias ElixirOpenapiMcp.Config.Config

  @doc """
  Loads the configuration from a file, environment variables, and CLI arguments.
  Returns `{:ok, config}` or `{:error, reason}`.
  """
  @spec load(map()) :: {:ok, Config.AppConfig.t()} | {:error, String.t()}
  def load(cli_args \\ %{}) do
    with {:ok, config_from_file} <- load_from_file(cli_args[:config]),
         config_with_env <- merge_env(config_from_file),
         config_with_cli <- merge_cli(config_with_env, cli_args),
         {:ok, validated_config} <- validate(config_with_cli) do
      {:ok, validated_config}
    end
  end

  defp load_from_file(nil) do
    {:ok, %{}}
  end

  defp load_from_file(config_file) do
    case File.read(config_file) do
      {:ok, body} ->
        case Yaml.decode(body) do
          {:ok, data} -> {:ok, data}
          {:error, reason} -> {:error, "Failed to decode YAML file: #{reason}"}
        end
      {:error, reason} ->
        {:error, "Failed to read config file: #{reason}"}
    end
  end

  defp merge_env(config) do
    Enum.reduce(System.get_env(), config, fn {key, value}, acc ->
      if String.starts_with?(key, "OPENAPI_MCP_") do
        parsed_key =
          key
          |> String.replace_prefix("OPENAPI_MCP_", "")
          |> String.downcase()
          |> String.to_atom()

        case parsed_key do
          :port -> Map.put(acc, parsed_key, String.to_integer(value))
          :timeout -> Map.put(acc, parsed_key, String.to_integer(value))
          :mode -> Map.put(acc, parsed_key, String.to_atom(value))
          :version -> Map.put(acc, parsed_key, value)
          :swagger_file -> Map.put(acc, parsed_key, value)
          :adjustments_file -> Map.put(acc, parsed_key, value)
          _ -> Map.put(acc, parsed_key, value)
        end
      else
        acc
      end
    end)
  end

  defp merge_cli(config, cli_args) do
    # CLI arguments have the highest precedence
    Map.merge(config, cli_args)
  end

  defp validate(config) do
    with :ok <- validate_swagger_file(config),
         :ok <- validate_mode(config) do
      server_config = %Config.ServerConfig{
        port: Map.get(config, :port, 3000),
        host: Map.get(config, :host, "127.0.0.1"),
        timeout: Map.get(config, :timeout, 30_000),
        mode: Map.get(config, :mode, :stdio),
        name: Map.get(config, :name, "Elixir OpenAPI MCP"),
        version: Map.get(config, :version, "0.1.0")
      }

      app_config = %Config.AppConfig{
        server: server_config,
        logging: %Config.LoggingConfig{}, # Placeholder, needs to be filled
        endpoint: %Config.EndpointConfig{base_url: Map.get(config, :endpoint, "")},
        swagger_file: Map.fetch!(config, :swagger_file),
        adjustments_file: Map.get(config, :adjustments_file),
        oauth: %Config.OAuthConfig{}
      }

      {:ok, app_config}
    else
      {:error, reason} -> {:error, reason}
    end
  end

  defp validate_swagger_file(config) do
    if Map.has_key?(config, :swagger_file) and config[:swagger_file] != "" do
      {:ok}
    else
      {:error, "Missing required configuration: swagger_file"}
    end
  end

  defp validate_mode(config) do
    mode = Map.get(config, :mode, "stdio")
    if mode in ["stdio", "http", "sse", :stdio, :http, :sse] do
      {:ok}
    else
      {:error, "Invalid server mode: #{mode}. Must be one of stdio, http, or sse."}
    end
  end
end
