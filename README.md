# mdmath-lsp

Small Rust language server for doing lightweight math inside Markdown in Helix.

`mdmath-lsp` is intentionally narrow. It does not try to be a notebook, spreadsheet, or symbolic algebra system. It watches for a `math:` line in a Markdown document, treats the rest of the file as math statements, and surfaces results through standard LSP features that Helix already supports well: diagnostics, hover, inlay hints, and code actions.

## What It Does

- Evaluates math statements after a `math:` line
- Supports arithmetic, variables, lists, and aggregate functions
- Supports column-style list input for summing or averaging figures
- Supports a built-in set of unit conversions
- Reports parse and evaluation errors inline in Helix

## Current Syntax

`math:` must appear at the start of a line. Everything before it is ignored.

```md
Notes above here are ignored.

math:
subtotal := 100
tax := subtotal * 0.07
subtotal + tax
```

### Expressions

Supported expressions:

- numbers like `12` and `19.95`
- arithmetic: `+ - * / ^`
- parentheses
- unary minus
- variable assignment with `:=`
- variable references
- list literals like `[1, 2, 3]`

Examples:

```md
math:
2 + 2 * 5
a := 10
b := a * 2
a + b
```

### Aggregate Functions

Supported functions:

- `sum(list)`
- `avg(list)`
- `min(list)`
- `max(list)`
- `len(list)`

Example:

```md
math:
nums := [1, 2, 3, 4]
sum(nums)
avg(nums)
min(nums)
max(nums)
len(nums)
```

### Column Lists

For note-taking, the most convenient list syntax is a named block that ends at the first blank line or end of file.

```md
math:
prices:
98.99
17.09
11.55

sum(prices)
avg(prices)
```

Bullet lists are also supported:

```md
math:
prices:
- 98.99
- 17.09
- 11.55

sum(prices)
avg(prices)
```

Each list item can also be an expression:

```md
math:
base := 10
prices:
base
base * 2
5 + 5

sum(prices)
```

Internally, a block like `prices:` is treated as a synthetic assignment such as `prices := [98.99, 17.09, 11.55]`.

### Unit Conversion

Supported unit families right now:

- length: `mm cm m km in ft yd mi`
- mass: `g kg oz lb`
- time: `s min h`
- temperature: `C F K`

Examples:

```md
math:
5 ft -> m
72 in -> cm
10 mi -> km
32 F -> C
100 C -> F
```

Invalid dimensional conversions are reported as diagnostics:

```md
math:
5 ft -> kg
```

## Helix Features

The server currently provides:

- diagnostics
- hover
- inlay hints
- code actions

Code actions include:

- replace an expression with its evaluated result
- insert the evaluated result after the expression

## Build And Run

Build the binary:

```bash
cargo build
```

Install it globally from this project with Cargo:

```bash
cargo install --path .
```

This installs `mdmath-lsp` into Cargo's bin directory, usually `~/.cargo/bin`. If you want Helix to launch it with `command = "mdmath-lsp"`, make sure that directory is on your `PATH`.

You can also run it directly over stdio:

```bash
cargo run --
```

## Helix Setup

Project-local `.helix/languages.toml`:

```toml
[language-server.mdmath-lsp]
command = "/Users/tim/developer/mdmath-lsp/target/debug/mdmath-lsp"

[[language]]
name = "markdown"
language-servers = [
  "marksman",
  { name = "mdmath-lsp", only-features = ["hover", "diagnostics", "inlay-hints", "code-action"] }
]
```

If `mdmath-lsp` is on your `PATH`, you can use `command = "mdmath-lsp"` instead.

For example, after `cargo install --path .`:

```toml
[language-server.mdmath-lsp]
command = "mdmath-lsp"

[[language]]
name = "markdown"
language-servers = [
  "marksman",
  { name = "mdmath-lsp", only-features = ["hover", "diagnostics", "inlay-hints", "code-action"] }
]
```

Project-local or user `config.toml`:

```toml
[editor.lsp]
display-inlay-hints = true

[editor]
end-of-line-diagnostics = "hint"

[editor.inline-diagnostics]
cursor-line = "warning"
```

## Example File

See `examples/demo.md` for a working sample.

## Limitations

Current limitations are intentional:

- `math:` must be at column 0
- everything after `math:` is treated as math mode
- assignment uses `:=`, not `=`
- expressions use `56 * 88`, not `56 * 88 =`
- list names must be valid identifiers like `prices` or `my_prices`
- currency prefixes like `$98.99` are not parsed yet
- no cross-file references
- no symbolic algebra

## Troubleshooting

If nothing appears in Helix:

1. Run `cargo build` so the binary exists.
2. Check `hx --health markdown` and confirm `mdmath-lsp` is found.
3. If needed, use an absolute `command` path in `.helix/languages.toml`.
4. Reopen the Markdown file or restart Helix after changing config or rebuilding.
5. Open Helix logs with `:log-open` if the server still does not appear to start.

## Development

Format, test, and check the crate with:

```bash
cargo fmt
cargo test
cargo check
```
