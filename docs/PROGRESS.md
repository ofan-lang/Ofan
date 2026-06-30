# Progress — Ofan

> Updated at the end of every working session with the agent. The next session starts by
> reading this file.

## Last session: 2026-06-29 — §19 Option/Checked types and variant names (docs/SYNTAX_SPEC.md)

**What was done:**
- Added §19 to SYNTAX_SPEC.md; renumbered former §19 deferred list → §20.
- Removed Option/Checked entry from deferred list.
- Updated §18 See also (§19 → §20), reserved-word table references, Self note.
- PR #13 merged (fast-forward). 48/48 tests pass (no code changes).

**Design decisions made (and why):**
- **`Option<T>` / `Some(T)` / `None`:** near-universal naming (Rust, Swift,
  OCaml, Scala). No deviation — pillar 2. `Maybe`/`Just`/`Nothing` rejected
  (Haskell-style, less intuitive for systems programmers); `Present`/`Absent`
  rejected (more verbose, no precision gain).
- **`Checked<T, E>` / `Ok(T)` / `Err(E)`:** type renamed from `Result` for
  pillar 1 — "Result" is semantically neutral; "Checked" signals programmer
  obligation to inspect the value. Variant names `Ok`/`Err` kept unchanged
  (pillar 2: near-universal). `Either<L,R>`/`Left`/`Right` rejected (no
  success/failure semantics in the variant names).
- **`Ok`, `Err`, `Some`, `None` are prelude constructors, not keywords.** No
  lexer changes. They lex as `Token::Ident`; type-checker gives them meaning.

**Pending / next step:**
- Enum declaration syntax — needed before `match` is fully useful; smaller
  session than `match` itself.
- `match` / pattern matching — plan-mode session; larger design decision.
- Parser: still waiting on at least `match` + enum before expression grammar
  can be implemented meaningfully.

**Something the agent proposed and was rejected (and why):**
- N/A.

---

## Last session: 2026-06-29 — §16 loop syntax spec (docs/SYNTAX_SPEC.md)

**What was done:**
- Confirmed deferred list is §19; §16 was an open gap between §15 and §17.
- Confirmed `break`/`continue` already reserved — no keyword gap found.
- Added §16 Loop syntax to SYNTAX_SPEC.md, filling the §15→§17 gap.
- Removed "Loop syntax" from §19 deferred list.
- Moved `loop`/`Token::Loop` in §19's reserved-words table from "ahead of
  syntax decisions" to "decided syntax (§16)"; updated keywords.rs comment
  to reference §16.
- Updated Contents table (§16 row), Status summary (18 of 19), anchor-link
  scan clean.
- PR #12 merged (fast-forward). 48/48 tests pass.

**Design decisions made (and why):**
- **`loop` as a distinct keyword from `while true { }`:** intent visible at
  the keyword itself (pillar 1 — explicit, not derivable only from reading
  the condition). Same principle as `&mut self` vs. unmarked mutation.
- **`break value` restricted to `loop` only:** `while`/`for` have two exit
  paths (explicit break + natural exit); `loop` has exactly one. Restricting
  `break value` to the form with a single exit path keeps the semantics
  unambiguous without requiring a decision about what the loop expression
  evaluates to on a natural exit.
- **`for` iteration forms inherit §7/§17/§18 model with no new mechanism:**
  `&`/`&mut`/bare-value at the iteration position — same pattern already
  locked for function parameters (§7), struct fields (§17), method receivers
  (§18). Named explicitly as the second validation that the ownership model
  generalizes cleanly across syntactically distinct positions.

**Pending / next step:**
- `Option`/`Checked` variant names — small design session; needed before
  parser can recognize success/error/absent patterns.
- `match` / pattern matching syntax — larger session, plan mode first.
- Parser: expression grammar, statement grammar, function definitions
  (plan mode first, per CLAUDE.md). Blocked on at least `Option`/`Checked`
  variant names.

**Something the agent proposed and was rejected (and why):**
- N/A.

---

## Last session: 2026-06-29 — keyword reservation pass (src/lexer/, docs/SYNTAX_SPEC.md)

**What was done:**
- Audited all §19 candidate words against actual `keywords.rs`; confirmed `while`, `for`,
  `in`, `enum`, `use`, `if`, `else`, `return`, `true`, `false` already reserved — not touched.
- Identified 4 decided-syntax gaps (§17/§18 words absent from keyword table): `copy`,
  `move`, `self`, `impl`.
- Identified 4 §19 future reservations: `loop`, `match`, `trait`, `mod`.
- Added 8 `Token` variants to `token.rs`, 8 entries to `keywords.rs`.
- Added 3 tests: decided-gap keywords, §19 reservation keywords, regression guard on
  pre-existing keywords. 48/48 pass.
- Updated `SYNTAX_SPEC.md` §19: replaced the "process note" about missing master
  reserved-word list with the actual master table.
- PR #11 merged (fast-forward).

**Design decisions made (and why):**
- Individual named token variants (`Token::Loop`, `Token::Match`, etc.) — not a generic
  `Token::Reserved`. A catch-all would push disambiguation into the parser; named variants
  let the parser pattern-match directly once grammar is decided.
- `Token::SelfKw` (not `Token::Self`) — avoids collision with Rust's own `self` keyword
  in the compiler source.
- `Self` (capital) intentionally not reserved — whether Ofan needs a `Self` type alias
  inside `impl` blocks is an open §19-adjacent question; flagged in SYNTAX_SPEC.md.

**Pending / next step:**
- §19 syntax decisions needed before parser work can start: loop forms (`loop`/`while`/
  `for`/`in` syntax and semantics), `match`/pattern matching, `Option`/`Checked` variant
  names. Recommend plan-mode sessions for each.
- `Self` (capital) reservation question — decide during the trait/impl design session.
- Parser: expression grammar, statement grammar, function definitions (plan mode first,
  per CLAUDE.md).

**Something the agent proposed and was rejected (and why):**
- N/A.

---

## Last session: 2026-06-28 — `IdentAfterNumericLiteral` lexer error (src/lexer/)

**What was done:**
- Investigated `1abc` behavior surfaced during §2 audit. Confirmed: ALL bases (decimal,
  float, hex, binary, octal) silently split on non-scannable characters, producing two
  tokens with no error.
- `0x1fg` identified as the sharpest case: `f` is a valid hex digit and silently consumed,
  leaving `g` as stray `Ident` — silent value corruption.
- `1_abc` inconsistency confirmed: already errors via `MisplacedDigitSeparator`; `1abc`
  did not. Both are "literal immediately abutting an identifier."
- Implemented `LexError::IdentAfterNumericLiteral { start, literal: String, ch }` in
  `error.rs`. Added helper `check_no_ident_follows` in `numbers.rs`; wired at all 3
  success return paths (hex/bin/oct integer, decimal float, decimal integer).
- 7 new tests (all 6 investigation-table cases + precedence test + operator/whitespace
  regression). All 45 tests pass.
- Updated `SYNTAX_SPEC.md` §14 with the rule, motivating case, and precedence note.
  Removed the item from §19 deferred list.

**Design decisions made (and why):**
- **Hard lexer error, not a parser-level error.** Since §14 already disallows literal
  suffixes, the input can never be valid. Lexer catches it with a message naming both
  the literal and the offending character; parser would produce generic "unexpected
  identifier" with no memory of the number. `0x1fg` value-corruption case is pillar 1.
- **First-problem-encountered precedence with `MisplacedDigitSeparator`.** The new check
  runs only after the number scan succeeds. `1_abc` still errors on `MisplacedDigitSeparator`
  (scan loop fails on `_` lookahead before reaching the success exit). Sequenced by
  control flow, not ranked by priority — stated explicitly in §14.

**Pending / next step:**
- Rust-idiom reviewer noted: if non-ASCII ident start ever added (§2 deferred), the
  `check_no_ident_follows` predicate (`is_ascii_alphabetic() || '_'`) would be a
  second source of truth. Route through the same classifier as the main lex dispatch
  arm at that point — no action needed now.
- Pillars reviewer noted: `0b102` / `0o19` (out-of-radix digit followed by more digits)
  still silently produces two `Integer` tokens. Same class of issue, different trigger.
  Worth a follow-up when the number scanner is next touched.
- Trait/interface syntax, parser grammar — §19, separate sessions.

**Something the agent proposed and was rejected (and why):**
- N/A.

---

## Last session: 2026-06-28 — §2 identifier character-set spec-gap closure (docs/SYNTAX_SPEC.md)

**What was done:**
- Investigated pillars-reviewer flag: identifier start vs. continuation character checks
  potentially inconsistent on Unicode.
- Read `src/lexer/mod.rs`: start check is `'a'..='z' | 'A'..='Z' | '_'` (char ranges,
  definitionally ASCII); continuation check is `c.is_ascii_alphanumeric() || c == '_'`
  (explicit ASCII method). Both consistently ASCII-only — no implementation bug.
- Confirmed via live lex output: `my_var` → `Ok`, `café` → `Err(UnrecognizedCharacter
  { byte: 3, ch: 'é' })`, `a_é` → `Err(UnrecognizedCharacter { byte: 2, ch: 'é' })`,
  `1abc` → `Ok([Integer(1), Ident("abc"), Eof])`.
- Determined root cause: §2's prior text said "letters, digits, and underscores" without
  defining "letters" as ASCII-only or Unicode-permitting — a spec gap, not an
  implementation bug.
- Amended §2 to state explicitly: ASCII-only (`a`–`z`, `A`–`Z`, `0`–`9`, `_`), with
  spec-gap-closure note naming the implicit implementation choice and confirming no code
  changes needed.
- Added deferred note to §2 (matching §15's pattern) for Unicode-permitting identifiers.

**Design decisions made (and why):**
- **ASCII-only identifiers — explicit, not just implicit.**
  - Phase 1 niche (microcontroller/no-std/speedcoding) is overwhelmingly ASCII source.
    Unicode complexity has no near-term payoff.
  - Unicode-permitting identifiers require two non-trivial sub-decisions — normalization
    form and confusable-codepoint handling (pillar 1) — that deserve their own pass.
  - Single-binary / no heavy toolchain (pillar 4) conflicts with embedding Unicode
    category tables.
  - Decision deferred, not rejected: §2's new deferred note is the entry point if
    Unicode-permitting identifiers are revisited.

**Pending / next step:**
- **`1abc` lexes as `Integer(1)` + `Ident("abc")` (two tokens, no error)** — logged in
  §19 (lexer-relevant deferred items) as a tracked open question. Decision needed before
  the parser is written; see §19 entry for the two options (keep as valid tokenization vs.
  hard lexer error).
- Trait/interface syntax — §19, separate session.
- Parser: expression grammar, statement grammar, function definitions (plan mode first).

**Something the agent proposed and was rejected (and why):**
- N/A — this was a pure spec-gap closure with no contested proposal.

---

## Last session: 2026-06-28 — §18 method receiver syntax (docs/SYNTAX_SPEC.md)

**What was done:**
- Added §18 Method receiver syntax to `docs/SYNTAX_SPEC.md` (PR #8, merged).
- Renumbered old §18 deferred list → §19. Updated Contents table, Status summary
  (16 of 17 → 17 of 18), §17 "See also" cross-reference (now points to resolved §18),
  and trait/interface syntax entry in §19 to reference §18.
- Removed "Method receiver syntax" from §19 deferred list — now resolved.

**Design decisions made (and why):**
- **Three receiver forms: `&self`, `&mut self`, `self` — no new syntax, all reuse
  existing mechanisms.**
  - `&self`: immutable borrow; caller's binding unchanged after call.
  - `&mut self`: mutable borrow; mutates receiver in place, caller's binding survives.
  - `self` (consuming): governed entirely by §17 Copy/Move rule — Move struct invalidates
    caller's binding; Copy struct leaves it untouched. No special-casing at method
    boundaries; same rule as ordinary function parameters.
  - Explicit `mut self` vs. `&mut self` disambiguation note added (pillar 5): one-char
    difference (`&`) is the entire signal of whether the caller keeps the binding.
  - `&mut self` as a distinct form is pillar 1: without it, mutation would require either
    consuming `self` (wrong semantics) or implicit unmarked mutation (forbidden).
  - §17 validation confirmed: Copy/Move rule generalizes to `self` with zero exceptions.

**Pending / next step:**
- Trait/interface syntax — how `impl` blocks interact with named traits; stays in §19.
- Ident Unicode start/continuation inconsistency (flagged by pillars-reviewer pass 1) —
  separate task, separate branch.
- Parser: expression grammar, statement grammar, function definitions (plan mode first).

**Something the agent proposed and was rejected (and why):**
-

---

## Last session: 2026-06-28 — §17 Copy/Move semantics (docs/SYNTAX_SPEC.md)

**What was done:**
- Added §17 Copy/Move semantics to `docs/SYNTAX_SPEC.md` (PR #7, merged).
- Renumbered old §16 deferred list → §18. Updated Contents table, Status summary
  (15 of 15 → 16 of 17), and method receiver entry in §18 to cross-reference §17.
- Removed "Copy vs. Move semantics" from §18 deferred list — now resolved.

**Design decisions made (and why):**
- **Move-by-default, compiler-inferred Copy, explicit `copy`/`move` override.**
  - Struct auto-infers Copy iff every field is recursively provably Copy (primitives or
    another auto-Copy struct). Otherwise Move.
  - `copy struct` / `move struct` prefix always overrides inference in either direction.
  - Heuristic warning (not error) when auto-inferred-Copy struct has a field named `fd`,
    `handle`, or `ptr`-prefixed. Fields named `id` explicitly excluded — too many false
    positives in game-dev / microcontroller plain-data structs.
  - Six alternatives rejected (see §17 rationale). Key asymmetry driving the decision:
    Copy-by-default's failure mode (forgot override on resource struct) → silent
    correctness bug; Move-by-default's failure mode (forgot `copy` on plain-data struct)
    → compile error. Pillar 1 forbids the first, tolerates the second.

**Pending / next step:**
- Method receiver syntax (`self`/`&self`/`mut self`) — stays in §18, informed by §17
  but not yet decided. Needs its own session.
- Ident Unicode start/continuation inconsistency (flagged by pillars-reviewer pass 1) —
  separate task, separate branch.
- Parser: expression grammar, statement grammar, function definitions (plan mode first).

**Something the agent proposed and was rejected (and why):**
-

---

## Last session: 2026-06-28 — PR #6 merge verification + branch cleanup

**What was done:**
- Pulled PR #6 (`refactor/split-lexer-modules`) into `main` — fast-forward merge.
- Verified all structural expectations from the PR:
  - `src/lexer/` contains `chars.rs`, `comments.rs`, `keywords.rs`, `numbers.rs`,
    `strings.rs` alongside `mod.rs`, `token.rs`, `error.rs`.
  - `mod.rs` has `push1`/`push2` helpers (no inline `tokens.push(...)` repetition).
  - `error.rs` has `MisplacedDigitSeparator { byte: usize }`.
  - `docs/SYNTAX_SPEC.md` §14 has correct placement-enforcement wording; old "ignored
    wherever they appear" text is gone.
- `cargo build`, `cargo clippy -- -D warnings`, `cargo test`: all clean.
- Test count: **38/38** (34 pre-existing + 4 digit-separator cases). Matches PR description.
- Deleted `refactor/split-lexer-modules` locally and on remote.

**Design decisions made (and why):**
- None. Sync-and-verify session only.

**Pending / next step:**
- Ident Unicode start/continuation inconsistency (flagged by pillars-reviewer pass 1) —
  separate task, separate branch.
- Parser: expression grammar, statement grammar, function definitions (plan mode first).

**Something the agent proposed and was rejected (and why):**
-

---

## Last session: 2026-06-26 (part 3 — lexer module split)

**What was implemented:**
- Structural refactor of `src/lexer/mod.rs` (~1013 lines) into per-construct submodules:
  - `comments.rs` — line/block/doc comment scanning
  - `numbers.rs` — decimal/hex/binary/octal + float scanning
  - `strings.rs` — string literal scanning with escape validation
  - `chars.rs` — character literal scanning with escape decoding
  - `keywords.rs` — keyword lookup table
- Shared `decode_escape` helper extracted to `mod.rs` (private). Both `strings.rs` and
  `chars.rs` call `super::decode_escape(delimiter, eof_err)` — the delimiter parameter
  keeps the escape sets correctly asymmetric (`"` vs `'`).
- Local variable `chars` renamed to `iter` in `lex()` to avoid shadowing the new `chars`
  submodule.
- Two additional tests added: `lex_string_reject_single_quote_escape` and
  `lex_char_reject_double_quote_escape` — lock in that the delimiter asymmetry is tested.
- `keywords::lookup` lifetime annotation simplified from named `'src` to elided `'_`.
- All 34 tests pass; `cargo clippy -D warnings` clean.

**Design decisions made (and why):**
- `decode_escape` placed in `mod.rs` (not a separate file) because it is a private helper
  shared only by two sibling submodules. Child modules can access private parent items via
  `super::`. Creating a sixth file for a 10-line helper would be over-decomposition.
- `chars.rs::scan_char` takes no `src: &'src str` parameter — char literal scanning
  decodes to a `char` value and does not need to slice the source.
- `pub(super)` on all submodule functions — they are not part of the crate's public API.

**Pending / next step:**
- Ident Unicode start/continuation inconsistency (flagged by pillars-reviewer pass 1) —
  separate task, separate branch.
- Parser: expression grammar, statement grammar, function definitions (plan mode first).

**Something the agent proposed and was rejected (and why):**
-

---

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
