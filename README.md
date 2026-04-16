# mdmath-lsp

A Markdown calculation LSP for Helix.

`mdmath-lsp` gives Markdown files two calculation modes:

- `math:` for freeform expressions, variables, lists, and conversions
- `sheet:` for spreadsheet-style Markdown tables with formulas and column aggregates

It is intentionally small and deterministic, but it is not just a math highlighter. It is a focused calculation engine for notes, estimates, and lightweight tabular analysis inside ordinary Markdown, surfaced through standard LSP features Helix already supports well: diagnostics, hover, inlay hints, and code actions.

## Install

Install from this project with Cargo:

```bash
cargo install --path .
```

Then add it to Helix:

```toml
[language-server.mdmath-lsp]
command = "mdmath-lsp"

[[language]]
name = "markdown"
language-servers = [
  { name = "mdmath-lsp", only-features = ["hover", "diagnostics", "inlay-hints", "code-action"] }
]
```

And enable inlay hints in Helix config:

```toml
[editor.lsp]
display-inlay-hints = true
```

If `mdmath-lsp` is not on your `PATH`, use an absolute binary path instead, or make sure Cargo's bin directory such as `~/.cargo/bin` is on your `PATH`.

## What It Does

- Supports two first-class Markdown calculation modes: `math:` and `sheet:`
- Evaluates freeform expressions, variables, list blocks, and unit conversions
- Evaluates spreadsheet-style Markdown tables with row formulas and column aggregates
- Supports common spreadsheet functions in both list and table workflows
- Reports parse and evaluation errors inline in Helix

## Modes

`math:` must appear at the start of a line. Everything before it is ignored.

`math:` can also include the first expression on the same line.

Markers inside fenced code blocks are ignored. After `math:` or `sheet:` is active, fenced code blocks are skipped and inline backticks can be used to escape parts of a line from evaluation.

Use `/math` or `/sheet` on their own line to close the active mode explicitly.

```md
Notes above here are ignored.

math:
subtotal := 100
tax := subtotal * 0.07
subtotal + tax
/math
```

```md
math: 2 + 2
3 + 3
```

`sheet:` must also appear at the start of a line. In sheet mode, normal math expressions still work, but Markdown tables gain spreadsheet-like formulas and column aggregation.

```md
sheet:
| Item        | Price | qty. | Total         |
| ----------- | ----- | ---- | ------------- |
| MacBook Pro | 1999  | 2    | =sum(B, qty)  |
| iPad        | 999   | 3    | =sum(Price,C) |

sum(Price)
avg(Total)
/sheet
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

### Functions

Supported functions:

- `sum(...)`
- `avg(...)`
- `min(...)`
- `max(...)`
- `len(list)`
- `count(...)`
- `product(...)`
- `median(...)`
- `abs(number)`
- `round(number)`
- `round(number, digits)`
- `floor(number)`
- `ceil(number)`
- `sqrt(number)`

`sum`, `avg`, `min`, `max`, `count`, `product`, and `median` accept either a list or spreadsheet-style arguments such as `sum(B, C)`.

`len` is list-only.

Example:

```md
math:
nums := [1, 2, 3, 4]
sum(nums)
avg(nums)
min(nums)
max(nums)
len(nums)
product(nums)
median(nums)
round(3.14159, 2)
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

### Sheet Tables

In `sheet:` mode, Markdown tables can contain formulas in cells that start with `=`.

Rules:

- `B` means the current row's second column, `C` the third, and so on
- if a header is present, you can also refer to that column by its header name
- after the table, header names become list variables for the whole column
- aggregate calls like `sum(Price)` and `avg(Total)` work on both literal and computed cells in that column

Example:

```md
sheet:
a := 10

| Item        | Price | qty. | Total         |
| ----------- | ----- | ---- | ------------- |
| MacBook Pro | 1999  | 2    | =sum(B, qty)  |
| iPad        | 999   | 3    | =sum(a, C)    |

sum(Price)
avg(Total)
```

Header names are normalized into identifiers. For example, `qty.` becomes `qty` and `Unit Price` becomes `Unit_Price`.

Plain expressions and assignments can also appear in `sheet:` mode outside tables.

### Escapes And Appended Results

Inline backticks escape text from evaluation after a mode is active.

```md
math:
a := 10 `comment`
a + 1
`sum(prices)`
```

Fenced code blocks are ignored inside both `math:` and `sheet:` modes.

````md
math:
a := 10
```python
this is ignored
```
a + 1
````

Math lines can include an appended answer after `=` and the evaluator will ignore that trailing result text.

```md
math:
2 + 2 = 4
subtotal + tax = 107
```

Sheet formula cells support the same pattern:

```md
sheet:
| Item | Price | qty. | Total                |
| ---- | ----- | ---- | -------------------- |
| iPad | 999   | 3    | =sum(B, qty) = 1002 |
```

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
- `sheet:` must be at column 0
- `/math` and `/sheet` must be on their own line
- everything after the most recent `math:` or `sheet:` marker stays in that mode
- fenced code blocks are ignored in both modes
- inline backticks escape text from evaluation
- only `math:` supports an expression on the same marker line
- assignment uses `:=`, not `=`
- appended result text like `2 + 2 = 4` is supported, but a bare trailing `=` like `56 * 88 =` is not
- list names must be valid identifiers like `prices` or `my_prices`
- currency prefixes like `$98.99` are not parsed yet
- sheet formulas only understand the current row plus variables already defined earlier in the document
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
