# brace 

**A JSON parser (RFC 8259) written from scratch, with no `serde_json`, `nom`, or `pest`.** Every syntax error comes back with the exact line and column where the input broke.

<img src="https://raw.githubusercontent.com/TheZoq2/ferris/master/rustacean-flat-happy.svg" height="60"/>

---

## Overview

`brace` is a JSON parser (RFC 8259) implemented in pure Rust. It doesn't depend on any parsing crate — `Cargo.toml` doesn't have a single line under `[dependencies]`, only the std. It takes an input string and returns an in-memory `Value` tree, or a `ParseError` that points exactly to where the syntax broke.

## Architecture

Two phases, like any recursive-descent parser:

```
&str ──> [Lexer] ──> Vec<Token> ──> [Parser] ──> Value
```

The `Lexer` scans the input character by character and emits tokens; it knows nothing about grammar, it only recognizes lexical units (`{`, `"key"`, `42`). The `Parser` consumes these tokens recursively, one method per grammar rule.

The non-obvious decision is in the `Token` type. The original spec described a simple enum (`LBrace`, `String(String)`, `Number(f64)`, ...) with no position at all. But the lexer finishes running before the parser starts — if the token doesn't carry where it began, that information is already lost by the time a grammar error shows up later. The fix was to separate the token's content (`TokenKind`) from its position: `Token` became a `TokenKind` wrapped with the `line`/`col` of that token's first character. That's what lets the parser, on hitting an unexpected token, copy the position straight from the token into the `ParseError` — it never needs to keep a position cursor of its own.

## Usage

```rust
use brace::{parse, Value};

let value = parse(r#"{"name": "brace", "version": 1}"#)?;

match value {
    Value::Object(pairs) => assert_eq!(pairs.len(), 2),
    _ => unreachable!(),
}
```

There's also a thin demo binary that reads from a file or stdin:

```bash
echo '{"a": [1, true, null]}' | cargo run
cargo run -- file.json
```

## Errors with position

This is the project's differentiator. Every invalid input produces a `ParseError` with exact line and column, counted starting at 1, not an approximate position.

```rust
let error = brace::parse("{\n  \"a\": ,\n}").unwrap_err();
println!("{error}");
```

Output (the crate's error messages are currently in Portuguese — this is real, unmodified program output):

```text
erro de sintaxe na linha 2, coluna 8: esperava valor, encontrou ","
```

`ParseErrorKind` has eight variants:

| Variant | When it occurs |
|---|---|
| `UnexpectedChar` | Character that doesn't start any valid token. |
| `UnexpectedToken` | The grammar expected one token and found another. |
| `UnterminatedString` | A string opened with `"` that never closes (including EOF in the middle of an escape). |
| `InvalidEscape` | A `\X` sequence where `X` isn't a recognized escape. |
| `InvalidNumber` | A number that doesn't follow RFC 8259's `number` grammar. |
| `UnexpectedEof` | Input ended where a token was expected. |
| `InvalidUnicodeEscape` | `\uXXXX` with a loose or incomplete UTF-16 surrogate pair. |
| `DepthLimitExceeded` | Object/array nesting above the allowed limit. |

## Conformance

The parser follows RFC 8259 strictly, which in some places is more restrictive than the most obvious reading of the grammar:

- Rejects forms that `str::parse::<f64>()` would accept but the RFC forbids: `01`, `1.`, `.5`, `+1`, `1e`. All of them return `InvalidNumber`.
- Rejects form feed, vertical tab, and BOM as whitespace between tokens. JSON only allows space, tab, `\n`, and `\r` — any other character there is `UnexpectedChar`.
- Accepts an escaped `\"` inside a string, but rejects literal newline and tab characters in the middle of a string (RFC 8259 forbids literal control characters in a string; the correct way is to escape them).
- Correctly combines UTF-16 surrogate pairs: `"😀"` becomes `"😀"`. A loose or incomplete surrogate is `InvalidUnicodeEscape`, it never silently falls back to `U+FFFD`.
- Enforces a nesting depth limit of 128 levels. A document with 100,000 open brackets in a row returns `DepthLimitExceeded` instead of blowing the call stack.

The JSONTestSuite ([nst/JSONTestSuite](https://github.com/nst/JSONTestSuite)) has been vendored and runs under `cargo test`. The fixtures live in `tests/fixtures/JSONTestSuite/test_parsing/` (318 files: 95 `y_`, 188 `n_`, 35 `i_`, with the original MIT `LICENSE` and a `PROVENANCE.md` describing origin and naming convention), and the harness is `tests/conformance.rs`. Result: 95/95 `y_` accepted, 188/188 `n_` rejected, 35 `i_` handled without crashing — zero crashes and zero timeouts across the 318 files. Nuance: 12 of the 188 `n_` files aren't rejected by the parsing logic itself — they contain invalid UTF-8 and are blocked at the conversion to `String`, before `brace::parse` is even called; the final verdict is correct, but the credit goes to Rust's `&str` type, not the parser. The other 176 `n_` files are rejected by real syntax errors; the harness distinguishes the two cases with a three-state `Veredito` enum (Portuguese for "verdict").

## Known limitations

- **`Number` is `f64`.** It loses precision for integers above 2^53: `9007199254740993` comes back as `9007199254740992`, with no error raised. This is silent loss, accepted as part of the spec from the start — not a bug to fix.
- **`1` and `1.0` collapse into the same `Value::Number(1.0)`.** The information about which form the original text used is lost during lexing and can't be recovered afterward.
- **`Object` is `Vec<(String, Value)>`.** It preserves key insertion order, but lookup is O(n) instead of O(1). Duplicate keys (`{"a":1,"a":2}`) are both kept, with no dedup — RFC 8259 leaves this case unspecified, and this is the project's deliberate choice.
- **The 128-level nesting limit rejects valid but very deeply nested JSON.** This is a deliberate choice to avoid blowing the real stack, explicitly allowed by RFC 8259 (which permits implementation limits), not an accidental side effect.
- **Out of scope**: JSON5/JSONC (comments, trailing commas, unquoted keys), incremental/streaming parsing, serialization (`Value` → string), arbitrary-precision numbers.
- **`test_transform/` from the JSONTestSuite isn't covered.** That directory of the suite tests how to transform ambiguous values (out-of-range numbers, duplicate keys), a different axis from what `test_parsing/` covers (accept/reject). Not vendored, not tested.

## Quality

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Current state: 117 tests passing — 54 unit tests (inside `src/`), 4 conformance tests in `tests/conformance.rs`, 35 in `tests/invalid.rs`, 23 in `tests/valid.rs`, plus 1 doctest. Clippy with no warnings.

## Project structure

```text
.
├── src/
│   ├── lib.rs      # public entry point: parse(), re-exports
│   ├── value.rs    # enum Value
│   ├── error.rs    # ParseError, ParseErrorKind
│   ├── lexer.rs    # Lexer, Token/TokenKind (private to the crate)
│   ├── parser.rs   # Recursive-descent parser (private to the crate)
│   └── main.rs     # thin demo binary (file or stdin)
├── tests/
│   ├── valid.rs         # integration via public API: inputs that should parse
│   ├── invalid.rs       # integration via public API: inputs that should fail, with position
│   ├── conformance.rs   # harness for the vendored JSONTestSuite: y_/n_/i_ by prefix
│   └── fixtures/        # vendored JSONTestSuite (test_parsing/, LICENSE, PROVENANCE.md)
└── docs/
    ├── project.md    # spec: grammar, data structures, scope
    └── planning.md   # implementation roadmap, technical decisions, and status
```
