defmodule ElixirOpenapiMcp do
  @moduledoc """
  The main entry point for the application.
  """

  alias ElixirOpenapiMcp.CLI

  def main(args) do
    case CLI.main(args) do
      {:ok, config} ->
        start_app(config)

      {:error, reason} ->
        IO.puts("Error: #{reason}")
        System.halt(1)
    end
  end

  defp start_app(config) do
    Application.start(:elixir_openapi_mcp, config)
    # Keep the main process alive
    Process.sleep(:infinity)
  end
end
