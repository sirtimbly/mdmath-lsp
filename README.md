# mdmath-lsp

Small Rust language server for live math inside Markdown in Helix.

## Features

- A line starting with `math:` switches the rest of the file into math mode
- Arithmetic, assignments, variables, lists, and `sum`/`avg`/`min`/`max`/`len`
- Column-style list blocks using `name:` followed by one item per line, optionally with `-` or `*` bullets
- Unit conversions like `5 ft -> m` and `32 F -> C`
- Diagnostics, hover, inlay hints, and code actions

## Run

```bash
cargo run --
```

## Helix

Project-local `.helix/languages.toml`:

```toml
[language-server.mdmath-lsp]
command = "mdmath-lsp"
args = ["--stdio"]

[[language]]
name = "markdown"
language-servers = [
  "marksman",
  { name = "mdmath-lsp", only-features = ["hover", "diagnostics", "inlay-hints", "code-action"] }
]
```

User `config.toml`:

```toml
[editor.lsp]
display-inlay-hints = true

[editor]
end-of-line-diagnostics = "hint"

[editor.inline-diagnostics]
cursor-line = "warning"
```

## Examples

````md
Notes above here are ignored.

math:
subtotal := 100
tax := subtotal * 0.07
subtotal + tax

nums := [1, 2, 3, 4]
sum(nums)
avg(nums)

figures:
- 12
- 18
- 9
- 21

sum(figures)
avg(figures)

5 ft -> m
````

Project-local `.helix/config.toml`:

```toml
[editor.lsp]
display-inlay-hints = true

[editor]
end-of-line-diagnostics = "hint"

[editor.inline-diagnostics]
cursor-line = "warning"
```
