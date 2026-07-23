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

### Building with the `codegen` feature (LLVM required)

The `codegen` feature compiles the LLVM backend (JIT tests, end-to-end compilation):

```sh
cargo build --features codegen
cargo test  --features codegen
```

**Windows path-with-spaces workaround:** if your LLVM install is under a path that
contains spaces (e.g. `C:\Program Files (x86)\LLVM-18.1.8\`), the `cc-rs` build
script inside `llvm-sys` splits the path at the space and produces a broken `-I` flag.
Fix: install LLVM to a space-free path and set `LLVM_SYS_181_PREFIX` before building:

```powershell
$env:LLVM_SYS_181_PREFIX = "C:\LLVM18"   # adjust to your actual install
cargo build --features codegen
```

To persist this automatically for the project, copy the provided template and adjust
the path for your machine:

```sh
cp .cargo/config.toml.example .cargo/config.toml   # gitignored — machine-local
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
