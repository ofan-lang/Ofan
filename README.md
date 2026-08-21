# Ofan

[![CI](https://github.com/ofan-lang/Ofan/actions/workflows/ci.yml/badge.svg)](https://github.com/ofan-lang/Ofan/actions/workflows/ci.yml)

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

Prerequisites: Rust (stable) and LLVM 18. Full setup, Windows workaround, and contribution workflow → [CONTRIBUTING.md](CONTRIBUTING.md)

```sh
cargo build                    # type-check only (no LLVM required)
cargo build --features codegen # full build — produces real binaries
cargo test --features codegen  # full test suite
```

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
| [docs/METHODOLOGY.md](docs/METHODOLOGY.md) | Feature development lifecycle, doc ownership, doc update-trigger rules |
| [docs/GIT_WORKFLOW.md](docs/GIT_WORKFLOW.md) | Branching, commit conventions, direct-push policy |

## What's not built yet

The compiler handles a real subset of the language today. Full list → [docs/ARCHITECTURE.md §"Not yet designed"](docs/ARCHITECTURE.md#not-yet-designed). Active roadmap → [docs/PROGRESS.md](docs/PROGRESS.md).
