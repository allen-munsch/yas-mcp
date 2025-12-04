defmodule ElixirOpenapiMcp.Server.ToolHandler do
  @moduledoc """
  Handles tool registration and execution.
  """

  alias ElixirOpenapiMcp.Requester.HttpRequester
  alias ElixirOpenapiMcp.Requester.Types.HttpResponse

  @doc """
  Handles the execution of a tool.
  """
  def handle_tool(route_config, app_config, tool_params, _frame) do
    with {:ok, %HttpResponse{status_code: status, body: body}} <-
           HttpRequester.execute(route_config, app_config, tool_params) do
      {:reply, body, %{status_code: status}}
    else
      {:error, reason} ->
        {:reply, %{error: reason}, %{status_code: 500}}
    end
  end
end
