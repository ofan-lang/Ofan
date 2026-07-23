# Ofan

A systems programming language with compile-time memory safety and a low learning curve.
Built in Rust, targeting LLVM.

**Status:** Pre-1.0, actively developed. The core compiler pipeline is complete
end-to-end — lexer, parser, typechecker, and LLVM codegen all work today, producing
real native binaries. Not production-ready; see [What's not built yet](#whats-not-built-yet).

## What it looks like

```ofan
struct Point { x: i32, y: i32 }

impl Point {
    fn score(self) -> i32 { self.x * 10 + self.y }

    fn shift(self, dx: i32, dy: i32) {
        self.x = self.x + dx;
        self.y = self.y + dy;
    }
}

fn main() -> i32 {
    let mut p = Point { y = 7, x = 3 };  # field order independent
    let v1 = p.score();                    # 37
    p.shift(2, -3);
    let v2 = p.score();                    # 54
    v1 + v2                                # implicit return: 91
}
```

Self-receiver mode (`&self`, `&mut self`, or by value) is inferred from the method body —
no annotations required. The full end-to-end smoke test (`examples/smoke_test.ofn`)
exercises structs, methods, recursion, all arithmetic and comparison operators, `while`,
`loop`, `if/else` as a value, and compound assignment, and exits with the correct checksum.

## CLI

```
ofan build <file.ofn> [-o <output>]   # compile to binary (default output: ./stem[.exe])
ofan run   <file.ofn> [-- <args>...]  # compile, run, forward exit code, clean up temp
ofan check <file.ofn>                 # type-check only — no LLVM required
```

`check` never calls LLVM and works without the `codegen` feature flag. Useful for editor
integration and CI where the full build chain is not available.

## Building

**Prerequisites:**

- Rust (stable toolchain)
- LLVM 18 — must be installed separately; not bundled

**Type-check only (no LLVM needed):**

```sh
cargo build
cargo test
```

**Full build with codegen** (required to actually compile `.ofn` files to binaries):

```sh
# Point LLVM_SYS_181_PREFIX at your LLVM 18 install, then:
cargo build --features codegen
cargo test --features codegen
```

> **Windows:** If LLVM is installed at a path containing a space (e.g.
> `C:\Program Files (x86)\LLVM-18.1.8`), the build script splits on the space and breaks.
> Install LLVM 18 to a space-free path (e.g. `C:\LLVM18`) and point `LLVM_SYS_181_PREFIX`
> there. The `.cargo/config.toml.example` file in the repo shows the recommended local
> config.

Full setup instructions, toolchain notes, and the contribution workflow →
[CONTRIBUTING.md](CONTRIBUTING.md)

## Design

Ofan targets the gap between "very safe but hard to read" (Rust) and "fast but manually
safe" (Zig). Core bets: automatic lifetime inference (no `'a` annotations in most code)
and explicit erroneous behavior — no silent undefined behavior; compile error if
detectable, documented runtime panic if not.

| Doc | Contents |
|-----|----------|
| [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md) | Design thesis, 5 non-negotiable pillars, semantic rationale |
| [docs/SYNTAX_SPEC.md](docs/SYNTAX_SPEC.md) | Token shapes, keywords, operators, literal forms |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Compiler phases, cross-cutting patterns, codegen decisions |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Session log, decision history, what's next |
| [docs/GIT_WORKFLOW.md](docs/GIT_WORKFLOW.md) | Branching, commit conventions, direct-push policy |

## What's not built yet

The compiler handles a real subset of the language today. Missing pieces:

- **Enum typechecking** — AST and parser complete; typechecker not started
- **Structs as field types** — a struct containing another struct hits a codegen gap
- **Generics in codegen** — generic functions defer through the pipeline; no lowering yet
- **Standard library / prelude** — no `Option<T>`, I/O, or `println!`
- **`for` / `match` / `?` / `as`** — parser complete; typechecking deferred
- **Traits and trait bounds**
- **Modules and namespaces** (`mod`, `use`)
- **C interop** — `extern` blocks decided and designed; not implemented
- **Lifetime annotations** — region inference is phase 2; `'a` syntax is reserved

See [docs/ARCHITECTURE.md § Not yet designed](docs/ARCHITECTURE.md#not-yet-designed) for
the full canonical list and [docs/PROGRESS.md](docs/PROGRESS.md) for the active roadmap.
