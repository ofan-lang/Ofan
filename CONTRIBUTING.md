# Contributing to Ofan

## Prerequisites

- **Rust** (stable channel) — install via [rustup](https://rustup.rs/)
- **LLVM dev libraries** — required by `inkwell` at build time.
  Check [inkwell's README](https://github.com/TheDan64/inkwell) for the currently
  supported LLVM version and install instructions for your platform.
  Common installs:
  - Ubuntu/Debian: `apt install llvm-<version>-dev`
  - macOS: `brew install llvm`
  - Windows: use the LLVM pre-built binaries from llvm.org and set `LLVM_SYS_<VER>_PREFIX`

## Build

```sh
cargo build            # debug
cargo build --release  # optimized
```

## Test

```sh
cargo test
```

## Format & lint

```sh
cargo fmt              # format (enforced — matches gofmt philosophy)
cargo clippy -- -D warnings
```

## Run

```sh
cargo run -- <file.ofn>
```

## Compiler internals layout

```
src/
├── main.rs          — CLI entry point, drives compilation pipeline
├── lexer/           — tokenizer (Lexer, Token)
├── parser/          — recursive-descent parser (Parser, ParseError)
├── ast/             — AST node types (Expr, Stmt, Decl)
├── typechecker/     — type inference + lifetime inference engine
└── codegen/         — LLVM codegen via inkwell
    └── llvm.rs
```

## Design decisions

Syntax decisions (token shapes, keywords, operators, literals) belong in
`docs/SYNTAX_SPEC.md`. All other language decisions (semantics, type system, memory model)
belong in `docs/PHILOSOPHY.md`. Read both before touching lexer/parser/type-checker/codegen.

## Workflow

See `CLAUDE.md` for the agent-assisted workflow (plan mode, pillars-reviewer, etc.).
Non-agent contributors: the same conventions apply — plan before large changes, never
commit failing tests.

Git and commit conventions → [docs/GIT_WORKFLOW.md](docs/GIT_WORKFLOW.md)
