# Ofan — Project instructions for Claude Code

A systems language with strong memory safety and a low learning curve.
See `/docs/PHILOSOPHY.md` for the full thesis — read it before any design decision about the
language itself (syntax, semantics, types). No need to re-read it for purely mechanical tasks
(formatting, tooling, CI).

## The 5 non-negotiable pillars
1. Explicit erroneous behavior, never silent UB (compile error if detectable; documented
   runtime panic if not).
2. Lifetime inference with opt-in escape hatch — the programmer doesn't annotate `'a` unless
   the compiler detects genuine ambiguity.
3. Single canonical syntax in shared source code; aliases only at write-time (editor/LSP),
   never as persisted ambiguity in the file.
4. Single-binary install, no heavy external toolchain.
5. Error messages always include context + a suggestion, never just "expected X, found Y".

If a proposed implementation violates any of these, flag it explicitly before proceeding,
even if nobody asked.

## Project commands
- Build: `cargo build` / `cargo build --release`
- Test: `cargo test`
- Format: `cargo fmt` (enforced; equivalent to gofmt — no style debates)
- Lint: `cargo clippy -- -D warnings`

## Conventions
- Every language design decision (not implementation detail) gets documented in
  `/docs/PHILOSOPHY.md` along with its reasoning, not just the outcome.
- Commits: imperative mood, explain the *why* of the change, not just the what.
- Never mark a task complete without showing evidence (real test output).

## Expected workflow
1. Plan mode first for any non-trivial feature — no code until the plan is approved.
2. Small steps with checkpoints ("do X, stop, show me") on large pieces (parser,
   type-checker, codegen).
3. Before closing a session: update `/docs/PROGRESS.md` with what was done, what was
   decided and why, and what's next.
4. After any non-trivial implementation in the lexer/parser/type-checker/codegen, invoke the
   `pillars-reviewer` subagent before committing.
5. Never commit if tests aren't passing.
