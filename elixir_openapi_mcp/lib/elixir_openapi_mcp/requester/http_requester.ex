defmodule ElixirOpenapiMcp.Requester.HttpRequester do
  @moduledoc """
  Makes HTTP requests.
  """

  alias ElixirOpenapiMcp.Requester.Types.{HttpResponse, MethodConfig, RouteConfig}
  alias ElixirOpenapiMcp.Config.Config.AppConfig
  alias Req

  @default_timeout 30_000

  def build_route_executor(route_config, %AppConfig{} = app_config) do
    fn params ->
      execute(route_config, app_config, params)
    end
  end

  def execute(%RouteConfig{} = route_config, %AppConfig{} = app_config, params) do
    base_url = app_config.endpoint.base_url

    url = build_url(base_url, route_config.path, params)
    method = route_config.method
    headers = route_config.headers
    req_options = build_req_options(route_config, params)

    case Req.request(method: method, url: url, headers: headers, receive_timeout: @default_timeout, options: req_options) do
      {:ok, %Req.Response{status: status, body: body, headers: resp_headers}} ->
        {:ok, %HttpResponse{status_code: status, body: body, headers: Map.new(resp_headers)}}
      {:error, reason} ->
        {:error, reason}
    end
  end

  def execute_direct(method, url, headers, body, params) do
    req_options = build_req_options_direct(body, params)

    case Req.request(method: method, url: url, headers: headers, receive_timeout: @default_timeout, options: req_options) do
      {:ok, %Req.Response{status: status, body: resp_body, headers: resp_headers}} ->
        {:ok, %HttpResponse{status_code: status, body: resp_body, headers: Map.new(resp_headers)}}
      {:error, reason} ->
        {:error, reason}
    end
  end

  defp build_url(base_url, path, params) do
    # Substitute path parameters
    substituted_path =
      Enum.reduce(params, path, fn {key, value}, acc ->
        String.replace(acc, "{" <> Atom.to_string(key) <> "}", to_string(value))
      end)

    "#{base_url}#{substituted_path}"
  end

  defp build_req_options(%RouteConfig{method_config: %MethodConfig{}} = route_config, params) do
    req_options = []

    # Handle query parameters
    query_params =
      Enum.filter(route_config.parameters, fn %{"in" => "query"} -> true end)
      |> Enum.map(fn %{"name" => name} -> {String.to_atom(name), Map.get(params, String.to_atom(name))} end)
      |> Enum.into(%{})

    req_options = Keyword.put(req_options, :params, query_params)

    # Handle request body (for POST, PUT, PATCH)
    if route_config.method in [:post, :put, :patch] do
      case Map.get(params, :body) do
        nil ->
          req_options
        body_content ->
          Keyword.put(req_options, :json, body_content)
      end
    else
      req_options
    end
  end

  defp build_req_options_direct(body, params) do
    req_options = []
    req_options = Keyword.put(req_options, :params, params)
    if body, do: Keyword.put(req_options, :json, body), else: req_options
  end
end
