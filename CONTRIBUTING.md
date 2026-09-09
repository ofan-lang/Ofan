# Contributing to Ofan

## Prerequisites

- **Rust** (stable channel) — install via [rustup](https://rustup.rs/)
- **LLVM dev libraries** — required by `inkwell` at build time.
  Check [inkwell's README](https://github.com/TheDan64/inkwell) for the currently
  supported LLVM version and install instructions for your platform.
  Common installs:
  - Ubuntu/Debian: `apt install llvm-<version>-dev`
  - macOS: `brew install llvm`
  - Windows: see the **Windows prerequisites** section below.

### Windows prerequisites

Two things are required on Windows in addition to Rust:

**1. LLVM static libraries**

The LLVM package from llvm.org does not include the static `.lib` files that `inkwell`
requires. Use [vovkos/llvm-package-windows](https://github.com/vovkos/llvm-package-windows)
instead. Download the variant that matches your compiler:

- Build type: **release** (not debug)
- C runtime: **msvcrt** (dynamic CRT — matches the Rust toolchain default)
- Architecture: **windows-amd64**
- MSVC version: match your installed Visual Studio (e.g. `msvc17` for VS 2022)

Extract the archive to a space-free path (e.g. `C:\LLVM18`) and set the environment
variable before building:

```powershell
$env:LLVM_SYS_181_PREFIX = "C:\LLVM18"   # adjust to your actual path
cargo build --features codegen
```

To persist this automatically for the project, copy the provided template:

```sh
cp .cargo/config.toml.example .cargo/config.toml   # gitignored — machine-local
```

**2. Visual Studio Build Tools with the C++ workload**

The Ofan compiler uses the MSVC linker (`link.exe`) to produce Windows binaries.
Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
(free) and select the **"Desktop development with C++"** workload.

The compiler locates `link.exe` automatically via the Windows registry — you do not
need to run `vcvars64.bat` or open a Developer Command Prompt.

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

## Design decisions

Syntax decisions (token shapes, keywords, operators, literals) belong in
`docs/SYNTAX_SPEC.md`. All other language decisions (semantics, type system, memory model)
belong in `docs/PHILOSOPHY.md`. Read both before touching lexer/parser/type-checker/codegen.

## Workflow

Feature development lifecycle → [docs/METHODOLOGY.md](docs/METHODOLOGY.md).
Agent-specific rules (plan mode, pillars-reviewer) → [CLAUDE.md](CLAUDE.md).

Non-agent contributors: the same conventions apply — plan before large changes, never
commit failing tests.

Git and commit conventions → [docs/GIT_WORKFLOW.md](docs/GIT_WORKFLOW.md)
