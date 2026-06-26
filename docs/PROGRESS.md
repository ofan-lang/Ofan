# Progress — Ofan

> Updated at the end of every working session with the agent. The next session starts by
> reading this file.

## Last session: 2026-06-26

**What was implemented:**
- Docs reorganization only. No src/ changes.

**Design decisions made (and why):**
- Created `docs/SYNTAX_SPEC.md` as the canonical home for Ofan's concrete syntax
  (keywords, operators, literal forms, token rules). Populated from lexer PRD session
  content; source preserved as `docs/prds/2026-06-26-lexer.md`.
- Split `docs/PHILOSOPHY.md` §5 from one "Resolved" block into three distinct subsections
  (5.1 implementation language & backend; 5.2 launch niche & sequencing; 5.3 C/C++ interop
  scope). Structural split only — no substance changes.
- Updated `CLAUDE.md` and `CONTRIBUTING.md` to point syntax questions to `SYNTAX_SPEC.md`
  and semantics/type-system questions to `PHILOSOPHY.md`.

**Pending / next step:**
- Apply lexer reviewer findings on `feat/lexer-implementation` branch:
  - Add `Copy + Hash` to `Span` (rust-idiom reviewer, medium)
  - Split `MalformedNumber { detail: String }` into typed variants (rust-idiom reviewer, medium)
  - Fix misleading `&`/`|` lone-character error message (both reviewers)
  - Fix dead `escape_pos` binding (rust-idiom reviewer, low)
  - Fix ident start/continuation Unicode inconsistency (pillars reviewer, low)
  - Decide `Token::Str/Ident` ownership: `String` vs. zero-copy `&'src str` (rust-idiom reviewer, medium — design decision)
- Then commit, run pillars-reviewer + rust-idiom-reviewer on final diff, open PR #4.
- Resolve §1 Comments and §2 Identifiers/casing in SYNTAX_SPEC.md (separate design session).

**Something the agent proposed and was rejected (and why):**
-

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
