defmodule ElixirOpenapiMcp.Parser.Adjuster do
  @moduledoc """
  Adjusts the parsed OpenAPI specification based on the adjustments file.
  """

  alias ElixirOpenapiMcp.Models.Adjustments

  def exists_in_mcp?(_path, _method, nil), do: true
  def exists_in_mcp?(_path, _method, %{"routes" => []}), do: true

  def exists_in_mcp?(path, method, %{"routes" => routes}) do
    Enum.all?(routes, fn %{"path" => route_path, "methods" => methods} ->
      not (match?(route_path, path) and (methods == [] or Enum.member?(methods, method)))
    end)
  end

  def get_description(_path, _method, default, nil), do: default
  def get_description(_path, _method, default, %{"descriptions" => []}), do: default

  def get_description(path, method, default, %{"descriptions" => descriptions}) do
    found_description =
      Enum.find_value(descriptions, fn %{"path" => desc_path, "updates" => updates} ->
        if match?(desc_path, path) do
          Enum.find_value(updates, fn %{"method" => update_method, "new_description" => new_description} ->
            if String.downcase(update_method) == String.downcase(method), do: new_description
          end)
        end
      end)

    found_description || default
  end

  defp match?(pattern, text) do
    Regex.match?(~r/^#{String.replace(pattern, "*", ".*")}$/, text)
  end
end
