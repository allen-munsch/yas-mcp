defmodule ElixirOpenapiMcp.Application do
  # See https://hexdocs.pm/elixir/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  alias ElixirOpenapiMcp.Config.Config.AppConfig
  alias Hermes.Server.Transport.StreamableHTTP

  @impl true
  def start(_type, config = %AppConfig{}) do
    children = [
      # Start the Hermes Server itself
      {ElixirOpenapiMcp.Server.Server, config},
      # Dynamically start the transport based on configuration
      transport_child_spec(config)
    ]

    opts = [strategy: :one_for_one, name: ElixirOpenapiMcp.Supervisor]
    Supervisor.start_link(children, opts)
  end

  defp transport_child_spec(%AppConfig{server: %{mode: :http, port: port}}) do
    {StreamableHTTP,
     http: [
       port: port,
       request_timeout: :infinity,
       log_handle_connection_errors: false,
       num_acceptors: 100
     ],
     server: ElixirOpenapiMcp.Server.Server}
  end

  defp transport_child_spec(%AppConfig{server: %{mode: :stdio}}) do
    # For stdio mode, Hermes.Server handles its own interaction
    :ignore
  end

  defp transport_child_spec(%AppConfig{server: %{mode: :sse, port: port}}) do
    # For SSE mode, we can use StreamableHTTP or a custom SSE transport if needed.
    # For now, let's assume StreamableHTTP can be adapted or will be replaced.
    {StreamableHTTP,
      http: [
        port: port,
        request_timeout: :infinity,
        log_handle_connection_errors: false,
        num_acceptors: 100
      ],
      server: ElixirOpenapiMcp.Server.Server}
  end
end
