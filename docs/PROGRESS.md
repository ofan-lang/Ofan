# Progress — Ofan

> Updated at the end of every working session with the agent. The next session starts by
> reading this file.

## Last session: 2026-07-12 — formalize self/Self design in SYNTAX_SPEC.md

**What was done:**

Updated `docs/SYNTAX_SPEC.md` to replace the old three-explicit-form method receiver
design (`&self`/`&mut self`/`self`) with the inference-based design settled in the
pillar-alignment session. Docs-only; no parser or typechecker code touched.

**Changes:**

- **§18 retitled** — "Method receiver syntax" → "Method receiver — `self` and `Self`"
- **§18 status line** — replaced: old decided statement referenced three explicit forms;
  new statement declares access mode inferred from body, `move self` as consuming override,
  `Self` as type alias
- **§18 code example** — replaced: `&self`/`&mut self` forms gone; bare `self` (inferred
  immutable/mutable) and `move self` (explicit consuming); `fn clone(self) -> Self` shows
  `Self` in type position
- **§18 prose** — rewritten around four topics:
  1. *Receiver access mode inference* — minimal borrow level determined from body usage,
     same mechanism as §17 Copy/Move; `&`/`&mut` not written in source (pillar 3)
  2. *`move self`* — explicit consuming override; mirrors `move struct` from §17
  3. *Ambiguity handling* — hard compile error with conflict-site pointing (pillar 1);
     error message format documented with example, must name conflicting usage sites (pillar 5)
  4. *`Self` type* — name-resolution alias for enclosing `impl` type; not a keyword token;
     unrelated to `self` receiver inference
- **§18 rationale** — pillars 3, 1, and §17 validation (third `move`/`copy` keyword position)
- **§17 "See also"** — updated: old reference to `&self`/`&mut self`/`self` three-way split
  → reference to Copy/Move inference mechanism extending to receiver access mode
- **§22 decided-syntax table** — `Self` row added (type namespace, not a token variant)
- **§22 open question** — `Self (capital) is not reserved` paragraph removed (resolved)
- **Status summary** — one-line note that §18 now also covers `Self`
- **Contents table** — §18 anchor updated to match new heading slug

**Decisions recorded:**

- `self` receiver annotation (`&self`/`&mut self`) does not exist in Ofan source —
  removed from spec entirely. Bare `self` is the only receiver form; access level is
  compiler-inferred.
- `move self` is the consuming override, completing the three-position validation of §17's
  `copy`/`move` override pattern.
- Inference ambiguity (conflicting body requirements) is always a hard compile error;
  never a silent fallback. Error must cite specific conflict sites, not just `self` param.
- `Self` resolves through the type namespace (not `Token::SelfKw`); no lexer change needed.

**Commit:** `228233c` (`docs: formalize self receiver inference and Self type alias in §18`,
direct to main, no PR — docs-only per workflow).

**Test and lint state:** 143 passed, 0 failed (unchanged — docs-only change).

**Pending / next steps:**
- **`SelfKw` lookahead fix in parser** — `try_parse_region_tag` in `src/parser/types.rs`
  has a `Token::SelfKw` inconsistency (known open item); now has a spec to implement against.
- **Typechecker phase 2: method/self resolution** — `impl` block design now spec'd; can
  begin binding `self` receivers and `Self` type to real types in the typechecker.
- **Typechecker phase 2: lifetime/region inference + Copy/Move enforcement** — partially
  blocked until method/self resolution is in place.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-12 — typechecker phase 1 (PR #21)

**What was done:**

Implemented the phase 1 typechecker: symbol table / scoping, literal typing, expression type
propagation, and block-tail inference. The pass produces a `HashMap<Span, Ty>` that codegen
will query. Files under `src/typechecker/`:

- `ty.rs` — `Ty` enum (primitives, `Ref`, `Named`, `Param`, `TyVar`/`Error` sentinels);
  `Region` (Named, Static); `FnSig`. Phase-2 hooks (`TyVar`, `Region::Var`) present but
  unreachable — avoids future breaking API change.
- `error.rs` — `TypeError` via `thiserror`. Phase-2 variants (`LifetimeConflict`,
  `UseAfterMove`, `BorrowConflict`) present for same reason. All fatal variants carry
  `suggestion: Option<String>` (pillar 5).
- `env.rs` — `Env` (scope stack, `push/pop/define/lookup`), `InferCtx` (fn signature
  table, `type_map: HashMap<Span, Ty>`, error accumulator). Phase-2 hooks for unification
  variables and region constraints left as commented-out fields.
- `infer/mod.rs` — two-pass entry: `collect_fn_sig` (pass 1, enables mutual recursion),
  `infer_fn`, `infer_block`, `defer`, `check_types`, tests.
- `infer/stmt.rs` — `infer_stmt`: `let`/`const`/`return`/`assign`/expression statements.
- `infer/expr.rs` — `infer_expr`, `infer_expr_inner`, `infer_literal`, `infer_call`.
- `infer/ops.rs` — `infer_unary`, `infer_binary` (full operator type tables).
- `infer/convert.rs` — `ast_ty_to_ty`, `ast_region_to_region`.

**Scope covered (phase 1 — in):**
- Primitive type resolution: `i32`, `f64`, `bool`, `char`, `str`, `unit`
- Literal typing; identifier resolution via `Env` scope stack
- Block-tail inference using `Block::tail` from PR #20 (`has_semicolon: false` → return value)
- `let`/`const` binding with optional annotation; `return`; simple assignment
- Unary (`-`, `!`, `~`, `&`, `&mut`) and binary (`+`, `-`, `*`, `/`, `%`, bitwise,
  comparison, logical) operators
- `if`/`else` with branch-type checking; `while`; `loop`
- Free function calls (ident callee, monomorphic): arg count + type checking
- Two-pass collection for mutual recursion

**Deferred (non-fatal `TypeError::Deferred` + `Ty::Error` sentinel):**
method calls, field access, cast, `?` operator, `for` loops, `match`, generic call
instantiation, compound assignments, `Self`/`self` receivers, user-defined types

**Phase-2 stability design:**
`Ty::Ref.region` is `None` in phase 1; `InferCtx` has commented-out fields for
`ty_var_count`, `ty_var_subst`, `region_constraints`; `InferResult` is opaque.
Adding region/unification fields in phase 2 changes only internals, not the public API.

**Method/self contact points — all flagged and deferred:**
`Expr::MethodCall`, `Expr::Field`, `Type::SelfTy` in `ast_ty_to_ty`, and `self`/`&self`
params in `bind_param` all emit `Deferred + Ty::Error`. Inference continues past all of
them without panicking.

**Agent reviews (two rounds each — initial + post-split):**

`pillars-reviewer` — no violations. Two advisories found and fixed before PR merge:
(1) unknown bare type names silently accepted as `Ty::Named` with no `Deferred` signal
    → `ast_ty_to_ty` now emits `Deferred + Ty::Error` for unresolvable names;
(2) generic instantiation arm discarded `defer()`'s return value and returned `Ty::Named`
    → changed to `return defer(...)`.
Post-split: confirmed no new pillar issues; no dropped error paths, no visibility leaks.

`rust-idiom-reviewer` — no blocking issues. Four findings addressed across both rounds:
(1) `InferResult` was dropping `Deferred` diagnostics on success → added `deferred` field;
(2) `ast_ty_to_ty`: `pub(crate)` → `pub(super)` (only called within `infer` module tree);
(3) `infer_call`: inline `crate::lexer::token::Span` path → top-level `use` import;
(4) compound assignments silently accepted → now explicitly deferred.
Deferred (acceptable for phase 1): `FnSig` clone per call in `infer_call` (borrow-order
constraint); repeated suggestion-formatting boilerplate in `#[error]` attrs.

**Submodule split (same PR):**
`infer.rs` (926 lines) split into the five files above, mirroring the parser's layout.
Pure reorganization — no logic changes, all 143 tests pass unchanged.

**Test and lint state (verified at merge):**
- `cargo test` — 143 passed, 0 failed.
- `cargo clippy -- -D warnings` — clean.

PR: **#21** (`feat/typechecker-phase1` → `main`, merged 2026-07-12).

**Phase 2 — deferred, not started:**
- Lifetime / region inference: `Ty::Ref.region` populated, constraint solving
- Copy / Move enforcement: move tracking, use-after-move errors
- Method / self resolution: **blocked on the `self`/`Self` trait-design session** —
  the impl-block design must be decided before method dispatch tables can be built;
  `Expr::MethodCall` and `Expr::Field` will remain `Deferred` until that session lands

**Known open items (carried forward):**
1. `try_parse_region_tag` `Token::SelfKw` lookahead inconsistency — belongs to
   `self`/`Self` trait-design session (unchanged from 2026-07-07 entry).
2. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.

**Pending / next steps:**
- **`self`/`Self` trait-design session** — unblocks method resolution (phase 2 typechecker)
  and closes the `SelfKw` lookahead inconsistency in the parser.
- **Typechecker phase 2** — lifetime/region inference + Copy/Move enforcement; partially
  blocked until trait-design session resolves method/self.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map; increasingly useful now
  that three phases (lexer, parser, typechecker) exist.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-11 — `has_semicolon` tail-expression fix (PR #20)

**What was done:**

Added `has_semicolon: bool` to `Stmt::Expr` so `{ foo() }` and `{ foo(); }` produce distinct
ASTs. Required before typechecker work begins: without it, return-type inference would need
to re-derive the tail/statement distinction from parser position rather than from the AST.

Changes:
- `src/ast/mod.rs` — `Stmt::Expr` gains `has_semicolon: bool`; doc comment encodes the
  invariant (`false` only appears transiently; `parse_block` always extracts it into
  `Block::tail`, never leaving it in `Block::stmts`).
- `src/parser/stmt.rs` — 2 construction sites updated; `parse_block` tail guard replaced
  from `peek() == RBrace` position heuristic to `has_semicolon: false` field match (cleaner:
  tail detection is now purely semantic, not lookahead-dependent); 4 tests updated/added.
- `src/parser/mod.rs` — `parse_block` `#[cfg(test)]` helper added.

`pillars-reviewer` — no pillar violations. One clarity note about `has_semicolon: false`
also firing at EOF (bare `parse_stmt("foo()")`): addressed in doc comment + new test.

`rust-idiom-reviewer` — no blocking issues. Two asks both addressed: (1) document the
transient-`false` invariant on the variant; (2) add a test for the EOF tail path
(`parse_expr_stmt_no_semicolon_at_eof`), which is the exact case where old and new
tail-detection logic diverged.

PR: **#20** (`fix/tail-expr-semicolon-field` → `main`, merged 2026-07-11).

**Test and lint state (verified at merge):**
- `cargo test` — 118 passed, 0 failed.
- `cargo clippy -- -D warnings` — clean.

**Resolves:** Known open item #2 from 2026-07-07 session.

**Pending / next steps:**
- **Typechecker implementation** — plan-mode session required (per CLAUDE.md); next major
  compiler phase. `has_semicolon` now gives the typechecker the signal it needs for
  return-type inference from block tail expressions.
- Remaining open items from 2026-07-07 session — see that entry below.

---

## Last session: 2026-07-07 — lexer + parser modularization shipped (PR #16, PR #19)

**What was done:**

Pre-commit work (audit + reviewer passes, required by GIT_WORKFLOW.md before any `src/` PR):
- Pre-commit audit surfaced `Token::SelfKw` inconsistency in `try_parse_region_tag` —
  logged to PROGRESS.md as an out-of-scope open item (see "Known open items" below).
- `pillars-reviewer` and `rust-idiom-reviewer` ran against both commit diffs.
- Four Pillar 5 violations found — bare `error_expected(..., None)` at `item.rs:9`,
  `expr.rs:221`, `pattern.rs:97`, `pattern.rs:144` — all fixed before committing.
- One rust-idiom finding: `impl<'src> fmt::Display for Token<'src>` in `token.rs` —
  elided to `impl fmt::Display for Token<'_>`.
- `parse_function` visibility narrowed from `pub(crate)` to `pub(super)` — only external
  caller is parent `mod.rs`; no crate-wide reach needed.
- Both reviewers re-ran on the final amended diffs: zero findings on either commit.

Commits landed on `main`:
- `3336734` — `refactor(lexer): modularize src/lexer/mod.rs into submodules`
- `fe82c05` — `feat(parser): implement parser, AST, and expression grammar across modular submodules`

PRs merged:
- **PR #16** (`feat/lexer-modularize` → `main`, merge commit `ccf8b00`) — lexer split.
- **PR #19** (`feat/parser-implement` → `main`, merge commit `32c3371`) — parser split.
  Supersedes the abandoned PR #17 (see incident note below).

**⚠ PR #17 incident — lesson learned:**
PR #17 was reviewed, approved, and merged into `feat/lexer-modularize` rather than
`main`. PR #16 had already merged `feat/lexer-modularize` into `main` before #17's
merge ran; GitHub's automatic base retarget did not occur. The parser commit (`fe82c05`)
landed on a branch that was already closed out, leaving `main` without the parser
changes. Fix: new PR #19 carrying the same reviewed commit directly onto `main`.

**Standing check (from this session forward):** before merging any PR whose base is a
feature branch, run `gh pr view <N> --json baseRefName` immediately before clicking
merge and verify the base is correct. Never assume GitHub has already retargeted a
dependent PR automatically.

**Test and lint state (verified on `main` at `32c3371`):**
- `cargo test` — 115 passed, 0 failed.
- `cargo build` / `cargo check` / `cargo clippy -- -D warnings` — all clean.

**Local state:** only `main` exists locally, tracking `origin/main` at `32c3371`.
Working tree clean. All feature and docs branches deleted (local and remote).

**Known open items (unresolved, not touched this session):**
1. `try_parse_region_tag` in `src/parser/types.rs` (~line 45–47): `Token::SelfKw` in
   `is_type_start` lookahead is inconsistent with `parse_type` having no `SelfKw` arm.
   Harmless in practice (still errors), but gives a misleading diagnostic on `&r1 self`.
   Belongs to the `self`/`Self`-in-type-position / trait-design session.
2. ~~Tail-expression bug: `Stmt::Expr` needs `has_semicolon: bool`.~~ **Resolved — PR #20 (2026-07-11).**
3. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code:
   `approx_constant` (PI) and `redundant_pattern_matching`. Not in the lint gate.

**Pending / next steps:**
- **`self`/`Self` trait-design session** — resolves the `SelfKw` lookahead inconsistency
  and decides how `Self` works in `impl` blocks.
- **Typechecker implementation** — plan-mode session required (per CLAUDE.md); the next
  major compiler phase.
- **`docs/ARCHITECTURE.md`** — high-level map of compiler phases, module boundaries, and
  data flow; useful before the typechecker grows large.
- **Anchor CLI tool** — per PHILOSOPHY.md §5.2; a real program to compile before the
  typechecker is complete, to validate language design against actual usage.
- ~~**Tail-expression `has_semicolon` fix**~~ — resolved in PR #20 (2026-07-11).

---

## Last session: 2026-07-04 — parser modularization + lexer modularization

**What was done (parser split):**
- Reviewed and modularized `src/parser/mod.rs` (~1395 lines) into 6 submodules:
  `item.rs`, `types.rs`, `stmt.rs`, `expr.rs`, `control_flow.rs`, `pattern.rs`.
- Folded 4 fixes in the same pass:
  1. `Token: Copy` added to `token.rs` derive — avoids `.clone()` on Copy payloads.
  2. `typechecker::infer` lifetime elision: `_ast: &Ast<'src>` → `&Ast<'_>`.
  3. `parse_type` written new during the split with no `Token::SelfKw` arm — `parse_type`
     never existed in committed code before this session, so nothing was dropped; the new
     implementation simply omits `SelfKw` by design, matching §18 (lowercase `self` is
     valid only as a value/receiver, not a type name; pillar 3).
  4. Removed dead `let _ = &ast;` from `main.rs`.
- Added `structural_suggestion` table in `Parser` and wired into `eat()` — structural
  token errors now always include a suggestion (pillar 5).
- Fixed trailing-comma handling for match arms (§21): moved the `}` check inside
  `parse_match_arm` so the last arm's comma is optional.
- Visibility strategy: cross-submodule method calls use `pub(crate)`; methods only
  called by parent `mod.rs` use `pub(super)`.
- All 115 tests pass; both pillars-reviewer and rust-idiom-reviewer ran clean.

**What was done (lexer split):**
- Finished lexer modularization: extracted the remaining inline logic from `src/lexer/mod.rs`
  (was 822 lines) into 3 new submodules + extensions to 5 existing ones:
  - `escapes.rs` — `decode_escape` moved from `mod.rs`; `pub(super)` (siblings access via
    `super::escapes::decode_escape`).
  - `operators.rs` — `scan_operator(ch, iter, pos) -> (Token<'static>, Span)` + 7 operator tests.
    All 14 operator chars (including `/` which was a standalone arm) unified into one dispatcher arm.
  - `punctuation.rs` — `lookup(ch) -> Option<Token<'static>>` table + new `lex_punctuation_table` test.
    10 punctuation arms collapse to a single `_` catch-all in mod.rs.
  - `keywords.rs` — `scan_identifier` added (loop consumes first char; no pre-consume in dispatcher).
  - `comments.rs`, `numbers.rs`, `strings.rs`, `chars.rs` — tests moved from mod.rs.
- `mod.rs` stripped to thin dispatcher + `push1` (private helper for punctuation arm) + 3 tests.
- Test count: 51 (50 existing + 1 new punctuation test); 115 total across the crate.
- Reviewer findings addressed: `decode_escape` narrowed from `pub(crate)` to `pub(super)`;
  `push2` deleted (dead code); `push1` kept private (children can see parent private items).
- Both pillars-reviewer (OK, no blockers) and rust-idiom-reviewer (3 WARNs, all fixed) ran clean.

**Design decisions made (and why):**
- `scan_identifier` called without pre-consuming the first char — unique among the scanner
  submodules (others pre-consume). The while loop's `peek()` call consumes the first char on
  its first iteration; this matches the original inline code exactly.
- `Token<'static>` return type from `scan_operator` and `punctuation::lookup` — operators
  and punctuation tokens hold no source slices; `'static` is the honest type and coerces into
  `Vec<(Token<'src>, Span)>` via covariance without annotation.
- Test `lex` helper (3 lines) repeated in each submodule rather than shared — self-contained
  test modules are a defensible pattern; the duplication is minor (rust-idiom-reviewer flagged
  as optional, not blocking).
- `push2` deleted rather than preserved: no caller remains after the split. The two-char span
  construction is inlined in `scan_operator` directly, which is simpler than threading a
  `tokens: &mut Vec` parameter through the return-value-based operator API.

**Pre-existing issue noted (not introduced by this session):**
- `cargo clippy --all-targets` (includes test code) flags `matches!(x, Ok(_))` → `.is_ok()`
  and `approx_constant` in `numbers.rs` test block. Not in the lint gate (`cargo clippy --
  -D warnings`), so this is a pre-existing residual. Fix in a separate pass.

**Pending / next step:**
- Commit parser split and lexer modularization work (two separate commits; no PR without
  explicit user go-ahead).
- Tail-expression bug: `{ foo(); }` and `{ foo() }` produce identical AST — `Stmt::Expr`
  needs `has_semicolon: bool`. Excluded from this pass per user's plan; needs a separate
  decision session.
- Pre-existing `--all-targets` clippy issues in `numbers.rs` (see above).
- **[Open — out of scope for this commit]** `try_parse_region_tag` in `src/parser/types.rs`
  (~line 45–47) includes `Token::SelfKw` in its `is_type_start` lookahead, so `&r1 self`
  would consume `r1` as a region tag then call `parse_type()` on `self` — which has no
  `SelfKw` arm and errors. The inconsistency is harmless in practice (still errors) but
  gives a misleading diagnostic. Found during pre-commit audit; not introduced deliberately.
  Belongs to the `self`/`Self`-in-type-position / trait-design session, not this pass.

**Something the agent proposed and was rejected (and why):**
- Reviewer suggested `decode_escape` as `pub(super)` alone (no re-export). Initially assessed
  as potentially broken for sibling access, but `pub(in super)` semantics cover descendants of
  the parent, so the suggestion was correct and applied.

---

## Last session: 2026-06-29 — §21 match / pattern matching syntax

**What was done:**
- Added `Token::FatArrow` (`=>`) to lexer (token.rs, mod.rs scanning, keywords.rs comment cleanup).
- Added §21 Match / pattern matching to `SYNTAX_SPEC.md`; renumbered deferred §21 → §22.
- Pillars reviewer found 5 issues; all resolved before commit.
- 50/50 tests pass. PR #15 merged (fast-forward).

**Design decisions made (and why):**
- **`match expr { arms }` — expression form.** No parens around subject (consistent with
  if/while/for). Evaluates to the matching arm's value, like `loop { break val }` (§16).
- **Arm separator `=>`** (`Token::FatArrow`) — only unallocated separator token (`:` is
  §9, `->` is §6, `=` is §5/§10).
- **Arm body: braceless single-expression canonical; braces for multi-statement.** §4's
  "mandatory braces" applies to control-flow block bodies; match arms are expression
  positions with `=>` + `,` as unambiguous boundaries. Formatter removes redundant
  braces on single-expression arms → one form in shared source (pillar 3).
- **Leading `|` on or-patterns: write-time alias.** Formatter removes it; no leading `|`
  in persisted source (pillar 3).
- **Binding vs. variant disambiguation: type-resolved** (§2 forbids casing enforcement;
  standard ML/Rust uppercase heuristic unavailable). Parser emits "ambiguous name" nodes;
  type-checker resolves. Unreachable-arm compile error closes the silent-logic-bug surface
  (if a mistyped variant becomes a catch-all binding, the arms below it are unreachable →
  compile error with suggestion).
- **Exhaustiveness: compile error** on enums (variants statically enumerable); compile error
  on open types (`i32`, `str`) without a `_` wildcard arm. Error messages name missing
  variants and suggest fixes (pillar 5).
- **`match` is the sole fallback for `Checked<T, E>`.** Consistent with §12/§19 — `?:`
  deliberately invalid on `Checked`; `match` forces naming the error case explicitly.
- **Deferred:** range patterns, `@`-binding, struct patterns (§20 struct variants deferred),
  slice patterns, or-pattern exhaustiveness with guards.

**Something the agent proposed and was rejected (and why):**
- Pillars reviewer (pass 1): found 5 issues, all fixed before commit. No user rejections.

**Pending / next step:**
- Parser: expression grammar, statement grammar, function definitions. Now unblocked on
  `match` + enums. Plan-mode session required (per CLAUDE.md). Recommend starting with
  expression grammar since that's the foundation everything else builds on.
- §22 remaining deferred: traits, modules, attributes, array/slice literals, generic call
  syntax, void/unit type.

---

## Last session: 2026-06-29 — §20 enum declaration syntax (docs/SYNTAX_SPEC.md)

**What was done:**
- Added §20 Enum declaration syntax to `SYNTAX_SPEC.md`; renumbered deferred list §20 → §21.
- Removed "Enum declaration syntax" from deferred list.
- Updated §18 See also (§20 → §21), reserved-word table references, Self note (§20 → §21).
- PR #14 merged (fast-forward). 48/48 tests pass (no code changes).

**Design decisions made (and why):**
- **Two variant forms: unit (bare name) and tuple (positional types in parens).** Unit
  variants cover boolean-style enums (`Direction::North`) and sentinel values. Tuple
  variants cover all payload cases for `Option<T>`/`Checked<T, E>` and user enums.
- **Struct variants deferred.** A tuple variant wrapping a named struct covers the
  use-case today with no expressiveness gap, only convenience. Revisit if real Ofan
  code shows the pain is consistently worth the surface-area cost.
- **`Ok`, `Err`, `Some`, `None` are prelude constructors, not keywords.** Lex as
  `Token::Ident`; no lexer changes. `Token::Enum` already reserved.
- **Copy/Move follows §17 rule unchanged — fourth validation.** Copy iff all variant
  payloads are provably Copy. `copy enum`/`move enum` prefix overrides inference.
  Heuristic warning cannot fire on positional tuple payloads (no field names);
  `move enum` required to override in that case. Named explicitly as the fourth
  validation that §17 generalizes across positions without special-casing.
- **Generic enums via `<T>` syntax (§7), no new mechanism.** `Option<T>` and
  `Checked<T, E>` are standard-library enums, not special compiler types.
- **`impl` blocks per §18, unchanged.** No new method syntax for enums.

**Pending / next step:**
- `match` / pattern matching — plan-mode session; larger design decision, now that
  enums are decided. This is the next logical step.
- Parser: expression grammar, statement grammar, function definitions (plan mode
  first, per CLAUDE.md). Blocked on at least `match` + enum (now unblocked for
  enum side).

**Something the agent proposed and was rejected (and why):**
- N/A.

---

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
