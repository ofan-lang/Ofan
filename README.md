# Ofan

A systems programming language with compile-time memory safety and a low learning curve.
Built in Rust, targeting LLVM.

**Status:** Pre-code — documentation and tooling phase. No working compiler yet.

## Design

Ofan targets the gap between "very safe but hard to read" (Rust) and "fast but manually
safe" (Zig). Core bets: automatic lifetime inference (no `'a` annotations in most code)
and explicit erroneous behavior (no silent undefined behavior).

Full design rationale → [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md)
Stack decisions and open questions → [docs/PHILOSOPHY.md §5](docs/PHILOSOPHY.md#5-technical-decisions)

## Get started

Prerequisites, build/test/run commands → [CONTRIBUTING.md](CONTRIBUTING.md)
Git and commit conventions → [docs/GIT_WORKFLOW.md](docs/GIT_WORKFLOW.md)

## Progress

Session log and decision history → [docs/PROGRESS.md](docs/PROGRESS.md)
