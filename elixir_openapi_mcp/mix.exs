defmodule ElixirOpenapiMcp.MixProject do
  use Mix.Project

  def project do
    [
      app: :elixir_openapi_mcp,
      version: "0.1.0",
      elixir: "~> 1.19.2",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      escript: [main_module: ElixirOpenapiMcp]
    ]
  end

  # Run "mix help compile.app" to learn about applications.
  def application do
    [
      applications: [:logger, :hermes_mcp],
      mod: {ElixirOpenapiMcp.Application, []}
    ]
  end

  # Run "mix help deps" to learn about dependencies.
  defp deps do
    [
      {:hermes_mcp, "~> 0.14.1"},
      {:optimus, "~> 0.5"}
    ]
  end
end
