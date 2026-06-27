# Progress — Ofan

> Updated at the end of every working session with the agent. The next session starts by
> reading this file.

## Last session: 2026-06-26 (part 2 — lexer implementation)

**What was implemented:**
- Full lexer on `feat/lexer-implementation`. Token set covers all of SYNTAX_SPEC.md §1–§15:
  - Keywords, identifiers (structural grammar, no casing enforcement — §2)
  - Operators: arithmetic, comparison, logical, bitwise, compound assignment,
    `?` (propagate) and `?:` (fallback) — §12
  - Comments: `#` line, `##...##` block, `###...###` doc (preserved as
    `Token::DocComment(&'src str)` for future tooling) — §1
  - Numeric literals: decimal/hex/binary/octal, `_` digit separators — §14
  - String literals with escape validation (hard error on unknown escapes) — §15
  - Character literals with same escape set (`\'` replaces `\"`) — §15
  - `unsafe` as reserved keyword — §13
  - `Token<'src>` is zero-copy (`&'src str` for `Str`, `Ident`, `DocComment`)
  - `Parser<'src>` lifetime propagated from zero-copy token change
- SYNTAX_SPEC.md: resolved §1, §2, §7 sub-item, §13; added §14 and §15.
  All 15 sections now decided. §16 collects deferred/undecided items.

**Design decisions made (and why):**
- `Token::DocComment` preserved (not discarded): §1 specifies forward-attachment
  semantics implying a downstream consumer; discarding is a one-way door. Parser
  can ignore if doc-gen never lands.
- `unsafe` keyword tokenized; block-scope enforced at parser level, not lexer
  level. `LBrace`/`RBrace` already provide block boundaries.
- Escape validation in lexer (hard error, per pillar 1), decoding deferred to
  later phase. Lexer returns raw `&'src str` slice (zero-copy compatible).
- `_` separators accepted in any position inside a numeric literal per §14's
  "ignored wherever they appear" wording. Position restrictions = new spec work.
- One commit for both reconciliation passes, not two: no committed checkpoint
  existed between them, so splitting would manufacture false history.

**Pending / next step:**
- Structural refactor of `src/lexer/mod.rs` (~980 lines) into per-construct
  submodules mirroring SYNTAX_SPEC.md sections: `comments.rs`, `numbers.rs`,
  `strings.rs`, `chars.rs`, `keywords.rs`. Shared escape-validation helper to
  be extracted (strings and chars share the same set). Pure move, no behavior
  change. Plan already drafted; waiting for both open PRs to land first.
- Ident Unicode start/continuation inconsistency (flagged by pillars-reviewer,
  pass 1) — separate task once refactor lands.
- Merge `docs/syntax-spec-and-philosophy-reorg` PR (#4) and
  `feat/lexer-implementation` PR (opened this session).

**Something the agent proposed and was rejected (and why):**
- Two-commit split for lexer work: rejected — both passes happened with no
  committed checkpoint between them; splitting would imply a sequence that
  never existed as committed state.

---

## Last session: 2026-06-25

**What was implemented:**
- Docs-only: resolved two previously pending §5.2 decisions in `docs/PHILOSOPHY.md`.

**Design decisions made (and why):**
- Launch niche / sequencing: lean cluster first (microcontrollers, speedcoding, game dev)
  before rich cluster (web/app dev). Rationale: easier to layer richness onto a lean core
  than to retrofit bare-metal constraints onto a runtime-heavy base. Anchor project: a
  real CLI tool, no mandatory OS/heap assumptions, even before microcontroller support.
- C/C++ interop v1: call-into-C only (no stable ABI embedding); C-only scope (C++ via
  C-shim layers only); mechanism is explicit `extern` block (Rust-style), not header
  parsing (`@cImport`-style). Rationale: maximally explicit FFI boundary (pillar 2.1),
  avoids building a C preprocessor before Ofan's own parser exists.

**Pending / next step:**
- Start lexer: token types + scanner (plan mode first, per CLAUDE.md workflow).
- Mascot character name — still pending artist collaboration.

**Something the agent proposed and was rejected (and why):**
-

---

## Last session: 2026-06-24

**What was implemented:**
- Project scaffold: CLAUDE.md, docs/PHILOSOPHY.md, docs/PROGRESS.md,
  .claude/agents/pillars-reviewer.md, .gitignore, CONTRIBUTING.md

**Design decisions made (and why):**
- Implementation language: Rust. Rationale: avoids writing a memory-safe compiler in
  an unsafe language (pillar 2.1); mature inkwell/melior LLVM bindings cover the need.
- Compilation backend: LLVM via inkwell. Rationale: multi-platform reach (x86, ARM,
  RISC-V, WASM) without per-arch codegen; Cranelift evaluated and rejected for lack of
  platform coverage and optimization maturity at this stage.

**Pending / next step:**
- Approve src/ scaffold, then create Cargo.toml + initial crate structure.
- Decide C/C++ interop mechanism.
- Decide concrete launch niche / anchor project.

**Something the agent proposed and was rejected (and why):**
-

---

## History
<!-- Previous sessions get moved here, most recent on top -->
