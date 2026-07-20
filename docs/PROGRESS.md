# Progress — Ofan

> Updated at the end of every working session with the agent. The next session starts by
> reading this file.

## Last session: 2026-07-19 — codegen PR 32 (branch + fixes, PR open for review)

**What was done:**

PR 32 code had been committed directly to `origin/main` without a branch or GitHub PR (workflow
gap from previous session). This session:
1. Force-reverted `origin/main` to `a4bbdb5` (pre-PR32 state).
2. Created branch `feat/codegen-pr32-control-flow-calls` and cherry-picked the two original commits.
3. Fixed three gaps identified by `pillars-reviewer` and `rust-idiom-reviewer`:

**Gap 1 — Two-pass function declaration (`lower_to_module`):**
- Added `declare_function_sig` to emit all LLVM function signatures (pass 1) before lowering any body (pass 2).
- Single-pass implementation broke any call where the callee appeared later in source order.
- T11 added to prove it: `fn fact5() -> i32 { fact(5) }` defined *before* `fn fact(n: i32) -> i32 { … }`. `fact(5) == 120`.

**Gap 2 — Recursive `emit_allocas`:**
- Added `emit_allocas_in_expr` which recurses into `If`/`While`/`Loop`/`Block` subtrees.
- Nested `let` allocas inside control-flow bodies are now hoisted to the entry block → mem2reg-eligible.
- Known limitation: shadowed names (same identifier at multiple nesting depths) reuse the outer alloca. Full scope-stack deferred to PR 33.

**Gap 3 — `@abort` `noreturn` attribute:**
- `get_or_declare_abort` now decorates the LLVM function with `AttributeLoc::Function + noreturn`.
- Without it, the `unreachable` instruction after `call @abort` was UB-shaped (LLVM could assume abort returns). Advisory finding from `pillars-reviewer`.

**All commits on branch; PR open, DO NOT MERGE — awaiting user review.**

**Test and lint state:** 213 passed, 0 failed. `cargo clippy --features codegen -- -D warnings` clean.

**Reviewer findings (post-fix):**
- `pillars-reviewer`: no blocking issues. Advisory: `fn abort()` user-name collision with div-zero guard — not user-reachable via CLI yet; fix before codegen is wired. Both advisories addressed (`noreturn` done; name-collision noted for PR 33).
- `rust-idiom-reviewer`: finding B (shadowing skip-guard is a latent aliasing bug, comment was misleading) — comment corrected, limitation documented; full fix is PR 33. Finding A (`FnLower` struct to replace `#[allow(too_many_arguments)]`) and check 6 (`Result<_, String>` → `CodegenError` enum) both deferred to PR 33.

**Pending / next steps (post-merge):**

- **PR 33 scope (next):**
  - Shadowed `let` bindings: scope-stack needed so inner shadows get their own alloca.
  - `FnLower<'ctx, 'src>` struct to eliminate `#[allow(clippy::too_many_arguments)]`.
  - `CodegenError` enum replacing `Result<_, String>` — needed for pillar 5 span-aware diagnostics.
  - `@abort` name-collision fix: use a reserved internal symbol (e.g. `__ofan_abort` or `llvm.trap`).
  - `loop { break value; }` (break with a value, loop-as-expression).
  - `Stmt::Assign` on non-ident targets (field write, index).
- **Slice 2 (structs/methods/fields):** follows slice 1 merge.
- **i32 literal range check in typechecker** (pillar 1 advisory from PR 31 review).
- **Integer overflow policy** — document wrapping/panic decision in PHILOSOPHY.md.

---

## Session: 2026-07-19 — codegen PR 32 control flow, calls, assignment, zero-divisor guard

**What was done:**

Landed PR 32 directly on `main` (commit `4041034`). Changed files: `src/codegen/llvm.rs` (full
extension), `src/parser/stmt.rs` (block-like semicolon elision).

**`src/codegen/llvm.rs` — new constructs lowered:**

- `CodegenEnv` now stores `(PointerValue, BasicTypeEnum)` pairs — eliminates re-querying
  `InferResult` on every load; required for compound assignment (`+=` etc.).
- `LoopCtx<'ctx>` struct (`break_bb`, `continue_bb`) threaded through `lower_stmt`/`lower_expr`.
- `unit_value(ctx) -> BasicValueEnum` sentinel for Ofan's `()` (typechecker guarantees no
  caller uses this in a value position).
- `lower_block` helper — stmt loop + tail-value return, used by `if`/`while`/`loop` arms.
- **Function parameters**: `lower_function` now builds `fn_type` with real param types; emits
  param allocas at entry-block top, stores `fn_val.get_nth_param(i)` after all alloca instructions.
- **`Stmt::Assign`** (plain `=` and compound `+=`/`-=`/`*=`/`/=`/`%=`): ident target only;
  compound loads via stored `BasicTypeEnum` then calls `lower_binary`.
- **`Stmt::Break`/`Stmt::Continue`**: unconditional branch to `LoopCtx::{break,continue}_bb`.
  `break` with a value returns a clear `Err` (deferred to later PR).
- **`Expr::If`**: cond branch → then_bb/else_bb → merge_bb. Value-producing if/else emits a
  phi node; Unit if omits it. `else_branch` is `Expr::Block` or `Expr::If` (else-if chains),
  both handled by recursive `lower_expr`.
- **`Expr::While`**: header_bb (cond) → body_bb/exit_bb. `LoopCtx` wires break → exit, continue
  → header. Env cloned for body scope so inner lets don't pollute outer.
- **`Expr::Loop`**: loop_bb ↔ exit_bb via `LoopCtx`. Implicit `build_unconditional_branch(loop_bb)`
  at body end if no explicit terminator.
- **`Expr::Block`**: inline `lower_block` with cloned env.
- **`Expr::Call` (Ident callee)**: `module.get_function(name)` + `build_call`; void calls return
  `unit_value`. Non-ident callees (closures, fn pointers) return explicit `Err` naming the
  PR they land in.
- **Zero-divisor guard for i32 div/mod** — two conditions, both route to `abort_bb`:
  1. `r == 0`: divide by zero.
  2. `l == INT_MIN && r == -1`: signed overflow → LLVM poison (pillar 1 — same standard as zero-div).
  Guard: `or(icmp eq r, 0, and(icmp eq r, -1, icmp eq l, INT_MIN))` → `abort_bb` or `ok_bb`.
  `abort_bb`: `call void @abort()` (libc `abort`, declared `external`), then `unreachable`.
  f64 div/mod: IEEE 754 defines behavior (±inf/NaN); no abort needed.

**`src/parser/stmt.rs` — block-like semicolon elision:**

Block-like expressions (`if`/`while`/`loop`/`for`/`match`/block) no longer require trailing `;`
as statements (consistent with Rust). Pillar-3 fix from `pillars-reviewer`: explicit `;` forces
`has_semicolon = true` (value discarded, expression stays in `stmts`); only semicolon-less
placement immediately before `}` promotes to block tail. The two forms now have distinct,
unambiguous semantics.

**JIT tests T6–T10:**

- T6: `fn add(a: i32, b: i32) -> i32 { a + b }` + `call_add()` → 8
- T7: `fn pick() -> i32 { if 1 < 2 { 10 } else { 20 } }` → 10
- T8: `fn countdown() -> i32 { let mut n = 3; while n > 0 { n = n - 1; } n }` → 0
- T9: `fn loop_break() -> i32 { let mut x = 0; loop { x = x+1; if x==5 { break; } } x }` → 5
- T10: IR-level: `fn divz() -> i32 { 10 / 0 }` LLVM IR contains `call void @abort` ✓

**Review:**

`pillars-reviewer` found two blocking issues before commit:
1. **Pillar 1**: `INT_MIN / -1` unsigned overflow missing from initial guard → fixed with two-condition
   check above.
2. **Pillar 3**: initial semicolon-elision consumed optional `;` but still promoted to tail —
   two spellings with identical semantics → fixed with `explicit_semi` flag.
Non-blocking noted: integer add/sub/mul wraparound policy should be documented in PHILOSOPHY.md
(wrapping is defined, not UB, but the design decision should be explicit); bare internal codegen
error strings get context when `CodegenError` enum lands (PR 33+).

**Test and lint state:** 212 passed, 0 failed. `cargo clippy --features codegen -- -D warnings` clean.

**Pending / next steps:**

- **Nested let scopes**: allocas for lets inside `while`/`loop` bodies are emitted inline
  (not at entry block top) — correct but not mem2reg-eligible. Full recursive hoisting + scoped
  env (push/pop) is a future optimization pass PR.
- **`loop { break value; }`** (break with a value) not lowered — deferred to a future PR when
  `loop` as an expression returning a concrete type is supported.
- **`Stmt::Assign` on non-ident targets** (field write, index) — deferred.
- **`CodegenError` enum** — `Result<T, String>` → typed enum, see open item 5 below.
- **i32 literal range check in typechecker** — see open item 4.
- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser + typechecker.
- **Integer overflow policy** — document wrapping/panic decision in PHILOSOPHY.md.

---

## Last session: 2026-07-19 — codegen PR 31 real AST lowering

**What was done:**

Replaced the PR 30 hardcoded stub with a full AST lowering pipeline. PR #31
(`feat/codegen-pr31-expr-lowering`, merged `d5f2684` fast-forward).

**`src/codegen/llvm.rs`** — full rewrite:

`LlvmContext::emit(&self, ast, types, out)` — new public entry point replacing
`emit_hardcoded_main`. Internal pipeline:
- `lower_to_module`: iterates `Ast::items`, dispatches `Item::Function` to `lower_function`.
  `Item::Struct`/`Item::Impl` skipped with comment; PR 32+ will lower these.
- `lower_function`: resolves return type via `basic_type_from_ast` (void when no annotation),
  builds LLVM function, pre-scans stmts via `emit_allocas`, lowers stmts, emits tail return.
- `emit_allocas` (pre-scan): walks all stmts before any arithmetic; emits one `build_alloca`
  per `Stmt::Let` at the top of the entry block → mem2reg-eligible (LLVM idiom).
- `lower_stmt`: `Let` (lower init + `build_store`), `Return` (`build_return`), `Expr` (discard);
  `_` arm returns explicit `Err` naming PR 32 for control flow and assignment.
- `lower_expr`: `Literal` (i32/f64/bool), `Ident` (load from env alloca), `Binary` (dispatch
  to `lower_binary`), `Unary` (neg/not); `_` arm returns explicit `Err` with byte offset.
- `lower_binary`: three arms — `Ty::I32` (sdiv/srem with PR-32-deferred zero-divisor note),
  `Ty::F64` (fdiv/frem with IEEE-754 note), `Ty::Bool` (and/or). `IntPredicate::SLT` etc.
  for signed comparisons; `FloatPredicate::OEQ` etc. for ordered float comparisons.
- `basic_type(Ty)` + `basic_type_from_ast(Type<'_>)` helpers for type mapping.
- Dead-code-after-return detection: `builder.get_insert_block().get_terminator().is_some()`
  breaks the stmt loop — semantic IR-level check, cannot be bypassed by syntactic wrapping.
- `emit_module` extracted from the old hardcoded method (target machine + link + .o cleanup
  — unchanged from PR 30).

**i32 literal range check (pillar-1 fix flagged by `pillars-reviewer`):**

`Literal::Integer(n)` lowering now range-checks `i32::MIN ≤ n ≤ i32::MAX` before
`const_int(*n as u64, true)`. Without this: `fn main() -> i32 { 3_000_000_000 }` lexes and
typechecks with no error but silently produces a wrong binary. Fix: explicit codegen error
with byte offset and range hint; no silent truncation.

Root source: `infer/expr.rs:132` types all `Literal::Integer(_)` as `Ty::I32` without a range
check. Typechecker-level fix (emit `Mismatch` at infer time with proper span + suggestion)
deferred to PR 32 where multi-width integers land.

**`src/typechecker/infer/mod.rs`** — typechecker fix for `ends_with_return`:

`fn main() -> i32 { return 42; }` was incorrectly rejected. Root cause: `infer_fn` (and
`infer_method`) compare the block's tail type (`Ty::Unit` when `Block::tail.is_none()`) against
the declared return type, without accounting for blocks that terminate via explicit `return` with
no tail. Fix: added `ends_with_return = body.tail.is_none() && last stmt is Stmt::Return` guard
suppressing the spurious `ReturnMismatch`. The `return` statement is independently type-checked in
`infer_stmt`. New test `explicit_return_satisfies_declared_return_type` covers both free functions
and methods.

**`src/main.rs`** — two-character change: `emit_hardcoded_main` → `emit` with `ast` and
`result` threaded through.

**JIT tests (T4, T5) — inkwell `ExecutionEngine`:**

`test_f64_arithmetic_jit`: source `fn f64_add() -> f64 { 1.5 + 2.5 }`, JIT-calls, asserts
result == `4.0_f64`.
`test_comparison_jit`: source `fn bool_cmp() -> bool { let a = 3; let b = 4; a < b }`,
JIT-calls as `unsafe extern "C" fn() -> u8` (LLVM `i1` zero-extends to low byte of `rax` on
x86-64 ABI), asserts result == `1u8`. No Cargo.toml change — `ExecutionEngine` always available
in inkwell 0.9.

**Manual binary tests:**

- T1: `fn main() -> i32 { 1 + 2 * 3 }` → exit **7** ✓
- T2: `fn main() -> i32 { let x = 10; let y = 3; x - y }` → exit **7** ✓
- T3: `fn main() -> i32 { return 42; }` → exit **42** ✓

**Review:**

- `pillars-reviewer`: initially NOT APPROVED — found silent i32 truncation (no range check on
  `Literal::Integer`). Fixed before PR merge: explicit codegen error for out-of-range literals.
  Div/mod-by-zero accepted as disclosed loud-crash (SIGFPE on x86-64), not silent UB — deferred
  to PR 32. Dead-code-after-return check confirmed semantic (IR-level, not syntactic — cannot be
  bypassed by wrapping). Approved after fixes.
- `rust-idiom-reviewer`: one structural note (not blocking) — `Result<T, String>` throughout
  codegen should become a typed `CodegenError` enum (already TODO'd at `llvm.rs:26`).
  `block_terminated` helper could DRY up the double `get_insert_block().and_then(...)` pattern.

**Test and lint state:** 207 passed, 0 failed. `cargo clippy --features codegen -- -D warnings` clean.

**Known open items — added this session:**

4. **i32 literal range check is in codegen, not typechecker.** `fn main() -> i32 { 3_000_000_000 }`
   now fails at codegen with a byte-offset error, but the typechecker passes it silently. A proper
   `Mismatch` at infer time (with span + suggestion to use a wider type when it lands) goes in with
   multi-integer-type support in PR 32+.

5. **`CodegenError` enum deferred.** `Result<T, String>` throughout `src/codegen/llvm.rs` should
   become a typed enum: at minimum `Unsupported { feature: &'static str, span: Span }`, `Internal(String)`,
   `Backend(String)`. Needed to carry span to `main.rs` for pillar-5-quality diagnostics. TODO comment
   at `llvm.rs:26`. Deferred to avoid scope creep.

**Known open items — carried forward (renumbered):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — `NB` comment at
   wildcard arm; blocked on §21 typechecker support.
3. **LLVM dev build targets X86+AMDGPU only** (carried from 2026-07-16 — see that entry for detail).
   Additionally: release build fails to link (WebAssembly target symbols missing from the local
   `(x86)` LLVM install). Dev builds use `profile.dev lto = "thin"` workaround.

**Known open item — div/mod by zero (disclosed):**

Integer division/modulo (`sdiv`/`srem`) trap on zero divisor as SIGFPE on x86-64 — loud crash, not
silent UB. `f64` division by zero yields IEEE 754 ±∞ (by spec, no trap). Runtime zero-divisor check
(icmp + branch to `abort`) deferred to PR 32 where control-flow lowering lands.

**Pending / next steps:**

- **PR 32** — control flow (`if`/`while`/`loop`) + function calls + runtime zero-divisor check.
- **`CodegenError` enum** — see open item 5; can land independently of PR 32.
- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser + typechecker.

---

## Last session: 2026-07-17 — codegen PR 30 infrastructure (first binary)

**What was done:**

Implemented end-to-end codegen pipeline and produced the first binary from an Ofan source
file: `fn main() -> i32 { 0 }` → `hello.exe` → exits 0. PR #30 (`feat/codegen-pr30-infrastructure`,
merged `f0cf83d` fast-forward).

**`src/codegen/llvm.rs`** — expanded from 20-line stub:

`LlvmContext::emit_hardcoded_main(&self, out: &Path) -> Result<(), String>`:
- `Target::initialize_x86` (x86-only for now; comment added)
- Build `define i32 @main() { ret i32 0 }` via inkwell builder API
- `TargetMachine::get_default_triple()` → `write_to_file` → intermediate `.o`
- `link_object` helper: tries `cc` → `clang` → `$LLVM_SYS_181_PREFIX\bin\clang.exe`
  (Windows fallback) in order; continues past non-zero exits (broken `cc` doesn't
  block working `clang`); retains last non-NotFound error for reporting
- Cleanup of `.o`: `eprintln!` warning on failure instead of silent `.ok()` (Pillar 1)
- TODO comment: promote to typed `CodegenError` enum for consistency with `TypeError`

**`src/main.rs`** — `Ok(result)` arm split into three parts:
1. `if result.has_deferred()` → print unsupported-construct diagnostics, exit 1
   (hard gate: no codegen node can be `Ty::Error`)
2. `#[cfg(feature = "codegen")]` → `LlvmContext::new()` + `emit_hardcoded_main(&out)` +
   print `"ofan: compiled → {out}"`
3. `#[cfg(not(feature = "codegen"))]` → existing not-yet-implemented message, exit 1

Removed crate-level `#[allow(dead_code)]`; replaced with targeted suppressions on
phase-2 items: `type_map`/`type_of` (PR 31+), `LifetimeConflict`/`UseAfterMove`/
`BorrowConflict` (phase-2 `TypeError` variants), `Ty::TyVar` (unification, phase 2).

**Review:**
- `pillars-reviewer`: APPROVED. Deferred gate airtight: every `Ty::Error` production site
  pairs with either a fatal `ctx.errors` entry or a `deferred` entry — no silent path to
  codegen. Pillar 4: system linker shell-out is a compile-action dependency, not an install
  dependency (consistent with documented design). Two carry-forward notes: (a) confirm
  release pipeline statically links LLVM; (b) `has_deferred()` gate unexercised until PR 31
  introduces real lowering — re-verify invariant then.
- `rust-idiom-reviewer`: Two issues found and fixed before commit:
  1. Silent `.ok()` on `.o` cleanup → explicit `eprintln!` warning
  2. Linker fallback stopped at first non-zero exit → try-all with last-error tracking

**Test and lint state:** 204 passed, 0 failed. `cargo clippy --features codegen -- -D warnings` clean.

**Verified end-to-end:**
```
ofan hello.ofn  →  ofan: compiled → hello.exe
./hello.exe     →  exit 0
```

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — `NB` comment
   at wildcard arm; blocked on §21 typechecker support.
3. **LLVM dev build targets X86+AMDGPU only** (carried from 2026-07-16 — see that entry
   for full detail).

**Pending / next steps:**

- **PR 31** — real AST lowering: primitive literals (`i32`, `f64`, `bool`), arithmetic
  operators, `let` bindings, `return`. Verification: binary exits expected value (e.g.
  `fn main() -> i32 { 1 + 2 * 3 }` exits 7).
- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser + typechecker.

---

## Session: 2026-07-16 — codegen kickoff design (docs)

**What was done:**

Expanded `docs/ARCHITECTURE.md` `## Codegen` section (commit `9c1006b`, direct to main)
with three settled design decisions from a pillar-alignment session.

**Decisions recorded:**

1. **LLVM static linking** — `ofanc` statically links LLVM (inkwell) into the
   distributed binary; no runtime dependency on a system LLVM. Concrete fulfillment of
   pillar 4 extended to "using the compiler." Dynamic linking rejected: reintroduces
   exactly the toolchain-fragmentation pillar 4 prevents, especially damaging for
   microcontroller/embedded targets.
   Pragmatic exception: shells out to the system linker (`cc`/`clang`/`link.exe`) for
   the final object-file → executable step. System C linkers are near-universally
   available in a way LLVM isn't — narrow, deliberate exception, not a violation of
   intent. Bundling lld noted as future option if the exception proves painful.

2. **First codegen slice scope** — slice 1 (first PR): primitive types/literals,
   arithmetic/comparison/logical operators, free function calls, `if`/`else`, `while`,
   `loop`, `let`, `return`. Structs/impl-block methods/field access deferred to slice 2.
   Rationale: isolates pipeline-plumbing risk (build system, object files, linking,
   target triples — first time any Ofan program produces a binary) from type-lowering
   risk (struct layout, ABI, method dispatch). Slice 1 maps cleanly to what the
   typechecker already fully resolves today.

3. **Ty::Error / Deferred gate** — hard structural check in `main.rs`: if
   `typechecker::infer()` returns `Err` or `InferResult::has_deferred()` is true,
   codegen is never invoked; driver prints which unsupported constructs are present and
   exits. One call site enforces the invariant — codegen lowering functions can assume
   every `Ty` is fully resolved. Documented as consistent with pattern §2
   (whole-program declaration-collection) and pattern §3 (tail-position transparency /
   pillar 1: stop and say so, never quietly degrade).

**Also done this session (docs):**

- Added cross-cutting pattern §4 (submodule-split precedent) to `docs/ARCHITECTURE.md`
  (commit `4d1cfb1`): three-question decision model, three precedent PRs (#21/#23/#29),
  navigation row added to "Where to look" table.
- Modularization health check: confirmed `infer/mod.rs` at exactly 1088 lines as
  predicted; no new files crossed the 300-line bar since last scan; no further
  splits warranted.

**Test and lint state:** no code changes; 204 passed, 0 failed (unchanged from PR #29).

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — `NB` comment
   at wildcard arm in `src/typechecker/infer/mod.rs`; blocked on §21 typechecker support.
3. **LLVM dev build targets X86+AMDGPU only.** The local development LLVM (vovkos
   llvm-package-windows 18.1.8, `C:\LLVM18`) only includes X86 and AMDGPU backends.
   This is sufficient for PR 30/31/32 (all host-native x86-64 development), but is a
   real gap relative to the LLVM-over-Cranelift decision, which was justified specifically
   by multi-platform reach including ARM/RISC-V for the microcontroller niche.
   Before any embedded/cross-compilation work begins, this needs a dedicated session to
   decide: (a) build a full-target LLVM for CI/release (required regardless — the
   distributed binary must support cross-compilation targets the dev convenience build
   doesn't); (b) confirm the dev/release LLVM split doesn't silently mask target-support
   gaps until someone tries to compile for ARM. `[profile.dev] lto = "thin"` in
   Cargo.toml works around the missing-symbol linker errors in debug builds by
   dead-stripping the unused `initialize_*` target functions from inkwell.

**Pending / next steps:**

- **Codegen slice 1** — implement per settled design above. First time an Ofan program
  produces a binary. Pipeline-plumbing focus: build system, inkwell wiring, object-file
  generation, system-linker invocation, driver loop changes in `main.rs`.
- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser +
  `Expr::StructLiteral` + typechecker field-count/name/type checking. Could land before
  or after codegen slice 1 depending on priority.

---

## Session: 2026-07-16 — infer/self_access.rs extraction (PR #29)

**What was done:**

Extracted the §18 self-access-mode scanning subsystem from `src/typechecker/infer/mod.rs`
into a new file `src/typechecker/infer/self_access.rs` (PR #29, commit `26c320f`, merged
to main `5a2d28c`). Pure file-move — no logic changed.

**Motivation:** `infer/mod.rs` reached 1286 lines after PR #27. The §18 scanning block
(198 lines: `infer_self_access_mode`, `SelfUsageScan`, all `scan_*` / `set_*` helpers)
is fully self-contained — pure AST tree-walk with no dependency on orchestration or
declaration-collection concerns. Extraction follows the exact precedent of the infer/
submodule split after PR #21.

**Changes:**
- `src/typechecker/infer/self_access.rs` created (202 lines).
- `src/typechecker/infer/mod.rs` 1286 → 1088 lines. Remaining bulk: collection passes
  + shared helpers + all tests. Health-check assessment: does not warrant further splitting.
- `infer_self_access_mode` visibility: `fn` → `pub(super) fn` (only caller is `mod.rs`).
- Dead `_self_span: Span` parameter dropped from `infer_self_access_mode` (never read;
  identified by `rust-idiom-reviewer`).

**Review:**
- `pillars-reviewer`: APPROVED — all 5 pillars N/A; pure move; tail-position transparency
  pre-existing and verified.
- `rust-idiom-reviewer`: no blockers; `pub(super)` correct; `_self_span` dead param cleaned.

**Test and lint state:** 204 passed, 0 failed; `cargo clippy -- -D warnings` clean.

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — `NB` comment
   at wildcard arm in `src/typechecker/infer/mod.rs`; blocked on §21 typechecker support.

**Pending / next steps:**

- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser + `Expr::StructLiteral`
  + typechecker field-count / field-name / field-type checking. Most natural next feature.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Session: 2026-07-16 — ARCHITECTURE.md + pillars-reviewer update

**What was done:**

Created `docs/ARCHITECTURE.md` (commit `ca36745`, direct to main — docs-only).
Updated `.claude/agents/pillars-reviewer.md` (commit `0aa6423`) with a standing
checklist item for tail-position transparency.

**`docs/ARCHITECTURE.md` contents:**

- Compilation pipeline: ASCII flow diagram + pointer to `main.rs` for wiring.
  Explicitly marks codegen as NOT YET IMPLEMENTED (exits 1 with message after typechecking).
- Per-phase sections (Lexer / AST / Parser / Typechecker / Codegen): entry points,
  key types, submodule tables, completeness status.
- Three named cross-cutting design patterns:
  1. **Inference-with-explicit-override** — same 3-case structure in §17 Copy/Move,
     §18 self receivers, §23 field access. `is_copy()` is the shared implementation.
  2. **Whole-program declaration-collection pass** — pass 1 sub-passes collect all
     names before pass 2 checks any body; future enum/trait/module support extends
     this pass.
  3. **Tail-position transparency (Pillar 1)** — named pattern distilled from Gap A
     (PR #27) and FieldOwnNonCopy (PR #28); pointer to `check_tail_field_own_non_copy`
     and the quick-test ("wrap in `{ … }` — does check still fire?").
- Not-yet-designed list (points at SYNTAX_SPEC.md §24 as canonical).
- Navigation table: "want to do X → look here."

**`.claude/agents/pillars-reviewer.md` update:**

Added explicit tail-position transparency checklist item: for any new
ownership/consumption check at a specific expression position, the reviewer must
verify it fires through `Expr::Block` tail, `Expr::If`/`else` branches, and future
`Expr::Match` arms. Names both historical instances (PR #27 Gap A, PR #28
FieldOwnNonCopy) as the rationale.

**Test and lint state:** 204 passed, 0 failed (no code changes).

**Resolved open items:**

- ✅ `docs/ARCHITECTURE.md` — **created this session** (was listed as pending in every
  session since PR #21).

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` — not in lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — `NB` comment
   at wildcard arm in `src/typechecker/infer/mod.rs`; blocked on §21 typechecker support.

**Pending / next steps:**

- **Struct literal construction** (`Point { x: 1.0, y: 2.0 }`) — parser + `Expr::StructLiteral`
  + typechecker field-count / field-name / field-type checking. Most natural next feature.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-16 — struct field access typechecking (PR #28)

**What was done:**

Implemented struct declaration parsing and field access typechecking (§23), completing
the `Expr::Field` deferred stub from the phase 1 typechecker and landing struct definitions
as first-class items across the parser, AST, and typechecker.

**Struct declaration AST + parser (`src/ast/item.rs`, `src/parser/item.rs`):**

`StructDef<'src>` added: `name`, `name_span`, `generic_params: Vec<&'src str>`,
`fields: Vec<FieldDef<'src>>`, `copy_move: Option<CopyMove>`, `span`. `CopyMove` enum:
`Copy` / `Move`. `Item::Struct(StructDef)` added — exhaustive match in typechecker passes
now has a `Struct(_) => {}` arm so future variants remain tripwires.

`parse_struct_def` in `item.rs`: optional `copy`/`move` prefix → `CopyMove`; `struct
Name`; optional `<T, U, …>` generic param list; `{ fields }` with `name: Type` entries
(trailing comma optional). `parse_item` updated; `parse_struct` helper in test module.

`SYNTAX_SPEC.md` §23 populated (was deferred): struct declaration syntax, `copy`/`move`
override, generic param list, field grammar. 184 lines of spec additions.

**Two-pass struct collection (`src/typechecker/infer/mod.rs`, `src/typechecker/env.rs`):**

Old single pass 1 became sub-passes 1a / 1b / 1c:

- 1a: `collect_struct_name` — register name + `name_span` into `ctx.struct_defs`; emit
  `DuplicateStruct` on collision, skip field population for duplicate.
- 1b: `collect_struct_fields` — populate `StructInfo.fields` (HashMap) and `field_order`
  (Vec for available-list ordering); resolves types with `convert::ast_ty_to_ty`. Two-pass
  design allows struct fields to reference other structs defined later in the file.
- 1c: existing fn/impl sig collection (unchanged).

`StructInfo` added to `env.rs`: `name_span`, `fields: HashMap<String, Ty>`,
`field_order: Vec<String>`, `copy_override: Option<CopyMove>`, `is_generic: bool`.
`InferCtx.struct_defs: HashMap<String, StructInfo>` added.

**`is_copy` helper (`src/typechecker/infer/mod.rs`):**

Implements §17 + §23 Copy-eligibility rule:
- Primitives (`i32`, `f64`, `bool`, `char`, `()`): always Copy.
- `&T` (shared ref): always Copy. `&mut T`: never Copy.
- `Ty::Named(name)`: check `struct_defs[name].copy_override` — `Some(Copy)` → Copy;
  `Some(Move)` → not Copy; `None` → recursive: all fields Copy iff all field types are Copy.
- `Ty::Str`, `Ty::Param`, `Ty::TyVar`, `Ty::Error`: not Copy.

**Field access typechecking (`src/typechecker/infer/expr.rs`):**

`infer_field_access` function (replaces the `Expr::Field` `defer()` call):
1. If object type is `Ty::Error` → cascade-suppress (no `FieldNotFound` piled on).
2. If object type is generic struct (`is_generic`) → defer (phase 1 limit).
3. Strip one `Ty::Ref` layer to find named struct type.
4. Look up field in `struct_defs[type].fields`; emit `FieldNotFound { available }` on miss.
5. Record field type in `ctx.type_map`.

**Ownership and mutability enforcement (`src/typechecker/infer/stmt.rs`):**

`Stmt::Assign` on `Expr::Field` target: if receiver is `Ty::Ref { mutable: false }` →
emit `FieldWriteViaSharedRef`; infer RHS and return early (no further type check). Falls
through for `&mut T` and owned receivers.

`FieldOwnNonCopy` checked at three call sites via `check_tail_field_own_non_copy`:
- `Stmt::Let` init: `let x = e.field` when struct is non-Copy.
- `Stmt::Return` value: `return e.field` when struct is non-Copy.
- Call args (expr.rs): `consume(e.field)` when struct is non-Copy and expected type is
  not a ref (ref-expected suppressed: type-mismatch fires instead).

**`check_tail_field_own_non_copy` (`src/typechecker/infer/mod.rs`):**

Recursive helper that follows transparent tail-position wrappers before checking
`check_field_own_non_copy`:
- `Expr::Field` → direct check.
- `Expr::Block` → recurse into `block.tail`.
- `Expr::If` → recurse into `then_block.tail` and `else_branch`; fires if either fires.
- `_ => false` — stops at non-transparent expressions (Unary, Binary, Call, etc.).

**Precondition:** `infer_expr` must have run on the full expression tree before this helper
is called, so `ctx.type_map` is populated for every `Expr::Field` span encountered.

**`infer_fn` and `infer_method` tail-guard (`src/typechecker/infer/mod.rs`):**

Bare function-body tail (`fn f(e) -> Sprite { e.sprite }`) is `f.body.tail` — an `Expr`,
not a `Stmt`. `infer_stmt` is never called on it; `Stmt::Return` does not catch it. After
`infer_block` returns, both `infer_fn` and `infer_method` now call
`check_tail_field_own_non_copy(f.body.tail)` and gate `ReturnMismatch` behind
`!tail_owns_non_copy` — ownership error is the root cause; the type error is noise.

These two call sites are separate (not DRY-merged) because they diverge on self-param
binding. Both are now symmetric for the tail-guard behavior. No third call site exists:
`infer_block` called from `infer_expr` (nested `Expr::Block`) has its value flow into a
containing `Stmt::Let` / `Stmt::Return` / call-arg — all three already covered.

**New `TypeError` variants (`src/typechecker/error.rs`):**

- `DuplicateStruct { name, first_span, duplicate_span }` — cites both sites; suggests rename.
- `FieldNotFound { type_name, field_name, span, available: Vec<String> }` — lists available
  fields sorted by definition order; empty-field case handled ("type has no fields").
- `FieldWriteViaSharedRef { type_name, field_name, span }` — names receiver kind, offers
  `&mut T` and owned-binding suggestions.
- `FieldOwnNonCopy { type_name, field_name, span }` — explains partial-move limitation,
  suggests borrow or whole-struct move.

**Tests (19 new in `src/typechecker/infer/mod.rs`):**

`ok_copy_field_read`, `ok_borrow_of_non_copy_field`, `error_field_own_non_copy_let`,
`error_field_own_non_copy_return`, `error_field_own_non_copy_call_arg`,
`error_field_write_via_shared_ref`, `error_field_not_found` (asserts field name AND
`available.contains(&"x")` — not just `errs.len() > 0`), `ok_cascade_suppression_on_error_receiver`
(asserts `errs.len() == 1` and variant is `UndefinedVariable`), `ok_copy_struct_override`,
`error_move_struct_override_non_copy_field`, `ok_mutable_ref_field_write`,
`deferred_generic_struct_field_access`, `error_field_own_non_copy_through_shared_ref_receiver`,
`error_field_own_non_copy_block_tail_let`, `error_field_own_non_copy_block_tail_return`,
`error_field_own_non_copy_block_tail_call_arg`, `error_field_own_non_copy_if_else_branches`,
`error_field_own_non_copy_implicit_return_bare`, `error_field_own_non_copy_implicit_return_if_else`,
`ok_field_copy_through_block_tail`, `ok_field_borrow_in_block_tail`, `error_duplicate_struct`.

**`Expr::Match` open item (documented, not a gap now):**

`check_tail_field_own_non_copy` hits `_ => false` for `Expr::Match` — each match arm is a
value-producing tail position. Flagged with a `NB` comment at the wildcard arm. Will need
to be added when §21 reaches full typechecker support. Currently non-fatal: match is still
deferred at the typechecker level, so the case cannot arise in practice.

**Two commits on this PR:**
- `ba7740e` — main struct parsing + field typechecking implementation.
- `33400a8` — `check_tail_field_own_non_copy` + `infer_fn`/`infer_method` tail-guard;
  call-arg sites upgraded from direct `Expr::Field` match to the recursive helper; 8 new
  tail-position regression tests.

**PR:** #28 (`feat/struct-field-typechecking` → `main`, merged 2026-07-16, fast-forward).

**Test and lint state:** 204 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Resolved open items:**

- ✅ `Expr::Field` deferred stub — **closed by PR #28** (field access now fully typechecked
  for non-generic concrete structs).
- ✅ Struct declaration syntax + typechecker — **closed by PR #28** (§23 struct parsing,
  two-pass collection, `StructInfo` table in `InferCtx`).

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.
2. `Expr::Match` arms not yet covered by `check_tail_field_own_non_copy` — flagged with
   `NB` comment at `mod.rs` wildcard arm; blocked on §21 typechecker support.

**Pending / next steps:**

- **Struct literal construction** — `Point { x: 1.0, y: 2.0 }` is not yet parsed or
  typechecked. Natural follow-on: parser support + `Expr::StructLiteral` + field-count /
  field-name / field-type checking in the typechecker.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-15 — method/self resolution (PR #27)

**What was done:**

Implemented method/self resolution in the typechecker, completing the impl-sigs pipeline
started in PR #26. Two commits on the branch — both reviewed and merged together.

**Self receiver binding (`src/typechecker/infer/mod.rs`):**

`move self` → `Ty::Named(type_name)` by value; no scan.

Bare `self` → body scan (`scan_self_usage`, `scan_block`, `scan_stmt`, `scan_expr`) before
type-checking classifies each `self` occurrence by syntactic position:
- `self` in free-function args, `let`/`return`/tail → CONSUMING
- `&mut self`, `self.field = ...` → MUTATING
- `self.method()` (object position) → NON-CONSUMING

Access mode resolved: CONSUMING → `Ty::Named`; MUTATING → `Ty::Ref { mutable: true }`;
else → `Ty::Ref { mutable: false }`.

`SelfAccessAmbiguity` hard error when the same body contains a consuming use AND any
borrowing use (non-consuming OR mutating). Predicate: `consuming.is_some() &&
(non_consuming.or(mutating)).is_some()`. Cites both conflict sites; never silently
falls through. Fixes two cases: (a) consuming+non-consuming (original spec), (b)
consuming+mutating (field assignment + move in same body — found by pillars-reviewer
and fixed in the same PR).

**`Self` in type position (`src/typechecker/infer/convert.rs`):**

`ast_ty_to_ty` gained `impl_type_name: Option<&str>` parameter. `Type::SelfTy` arm:
if `Some(name)` → `Ty::Named(name.to_string())` (no Deferred); if `None` → Deferred
error (top-level context, Self has no meaning). All non-impl call sites pass `None`.

**Method call dispatch (`src/typechecker/infer/expr.rs`):**

`Expr::MethodCall` stub replaced with `infer_method_call`:
1. Type receiver via `infer_expr`
2. `Ty::Error` receiver → cascade-suppress (infer all args, return `Ty::Error`, no
   second error)
3. `dispatch_type_name` — strips one `Ty::Ref` layer to find `Ty::Named` for dispatch
   (handles bare-self methods where `self: &Foo`)
4. `impl_sigs[type_name][method]` lookup
5. `sig.self_consuming && recv_ty is Ty::Ref` → `TypeError::ConsumeViaRef` (see Gap A)
6. Generic method → defer
7. Arg count check → `ArgCountMismatch`
8. Per-arg type check → `Mismatch`
9. Return `sig.return_ty`

`MethodNotFound` lists available methods sorted when the type has an impl block, uses
`Display` (not `{:?}`) for receiver type in messages. `Display` impl added to `Ty`.

`infer_all(args, ctx, env)` helper extracted to replace fivefold duplicated arg-drain
pattern.

**Gap A — consuming method called through reference (`src/typechecker/error.rs`):**

Found in pre-merge clarification pass. `move self` method callable through `&Entity`
with no error — type-level violation (cannot move out of a borrow), detectable without
lifetime machinery. Fix:

- `FnSig.self_consuming: bool` restored (had been dropped post-review as "dead" — but
  the use case was identified; the choice to drop rather than wire was premature)
- `collect_impl_sigs`: extracts `self_consuming = p.consuming` from the self param
- `infer_method_call`: after sig lookup, before arg checks — if `sig.self_consuming &&
  Ty::Ref { .. } = recv_ty` → `TypeError::ConsumeViaRef { type_name, method_name, span }`
- `ConsumeViaRef` message: names the method, states receiver is a reference, offers
  two suggestions (call on owned value, or remove `move self`)
- Covers both `&T` and `&mut T` (both are `Ty::Ref`)

**New `TypeError` variants (`src/typechecker/error.rs`):**

- `MethodNotFound { type_name, method_name, span, suggestion }` — method not in impl
  namespace or type has no impl block; suggestion lists available methods
- `SelfAccessAmbiguity { fn_name, consuming_span, other_span }` — §18 hard error;
  cites both sites; never silently resolves
- `ConsumeViaRef { type_name, method_name, span }` — move-self method called through
  a reference receiver

**`FnSig` changes (`src/typechecker/ty.rs`):**

- `self_consuming: bool` — true when `move self`; false for free functions and bare-self methods
- `Display` impl added to `Ty` for user-readable type names in error messages

**Tests (12 new, `src/typechecker/infer/mod.rs`):**

`error_method_not_found_on_primitive`, `ok_method_call_returns_type`,
`error_method_not_found_wrong_name`, `error_method_arg_count_mismatch`,
`ok_move_self_binds_by_value`, `error_self_access_ambiguity`, `ok_method_cascade_suppression`,
`ok_self_return_type_resolves`, `ok_self_ref_receiver_dispatch`,
`error_method_arg_type_mismatch`, `error_self_mutating_and_consuming_ambiguity`,
`error_consume_via_ref` (asserts `errs.len() == 1` — ConsumeViaRef does not cascade).

**Pre-merge clarification pass (two rounds of reviews, two commits):**

Commit 1 (`a6518d3`): original implementation. Pillars-reviewer found the §18
consuming+mutating ambiguity gap (predicate widened) and noted dead `has_self_receiver`/
`self_consuming` fields. Rust-idiom-reviewer found the `{:?}` Debug blob in MethodNotFound
and the fivefold arg-drain duplication. Both fixed before committing.

A post-commit clarification Q&A against §18/§22 surfaced Gap A (consuming-through-reference
silently allowed — `self_consuming` was dropped in the clean-up but its only real use case
was exactly the ConsumeViaRef check).

Commit 2 (`b58bebc`): Gap A fix + 3 new tests. Both reviewers re-ran on the delta and
approved. Pillars-reviewer confirmed both `&T` and `&mut T` covered, `self_consuming`
cannot be spuriously true (parser guarantees `consuming` only for `move self`), Pillar 5
satisfied. Rust-idiom-reviewer found no blockers; suggested tightening the ConsumeViaRef
test to also assert `errs.len() == 1` (done before push).

**`bind_param` note (not a gap, documented):**

`bind_param` still contains a `Type::SelfTy` arm that emits Deferred — this fires only
for a `self` param on a top-level `fn` (syntactically odd but syntactically legal). Methods
route through `infer_method` before `bind_param` is reached. Self-parameter logic now
lives in two places with different behavior; future sessions should not assume `bind_param`
is the single source of truth.

**Phase 2 scope (not addressed, not a gap):**

Double-consume and consume-while-simultaneously-borrowed require move tracking and borrow
checking. `UseAfterMove` and `BorrowConflict` placeholder variants already in `error.rs`;
this is explicitly Phase 2 scope.

**PR:** #27 (`feat/method-self-resolution` → `main`, merged 2026-07-15).

**Test and lint state:** 175 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Resolved open items:**

- ✅ `Expr::MethodCall` deferred stub — **closed by PR #27**.
- ✅ `Type::SelfTy` always Deferred in `ast_ty_to_ty` — **closed by PR #27** (`Self` in
  impl context now resolves to `Ty::Named`).
- ✅ `bind_param` defers `self`/`move self` to `Ty::Error` — **closed by PR #27**
  (methods route through `infer_method` instead).

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.

**Pending / next steps:**

- **`Expr::Field` resolution** — field access still deferred; requires struct field table
  (struct definitions not yet parsed/stored). Natural follow-on once struct declaration
  lands.
- **Struct declaration syntax + typechecker** — `Type::Named` for user types currently
  defers to `Ty::Error`; struct definitions needed to populate a type table and unblock
  field access, method arg types involving user-defined types, and proper `Ty::Named`
  resolution.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-15 — §22 impl-block merge + conflict detection (PR #26)

**What was done:**

Closed two pillar-1 gaps in one declaration-collection pass in the typechecker.

**`collect_fn_sig` fix (`src/typechecker/infer/mod.rs`):**

Pre-existing gap: bare `HashMap::insert` silently overwrote on duplicate top-level
function names (documented in §22 rationale as a known issue). Fixed: check
`ctx.fn_sigs.get(f.name)` before inserting; on collision emit `TypeError::DuplicateFn`
citing both definition spans and return early (keep first definition).

**Impl-block merge + duplicate detection (`src/typechecker/infer/mod.rs`):**

New `collect_impl_sigs` function populates `ctx.impl_sigs: HashMap<String,
HashMap<String, (FnSig, Span)>>` (outer = type name, inner = method/assoc-fn name).
Per §22's merge rule — all `impl TypeName` blocks in the program form one namespace —
any duplicate method name across blocks emits `TypeError::DuplicateMethod` citing both
definition spans, naming the type, and explaining the merge rule. Pass 1 loop updated
to dispatch `Item::Impl(block) => collect_impl_sigs(block, &mut ctx)`. Pass 2 still
defers method body checking (`Item::Impl(_) => {}`).

**Borrow pattern in `collect_impl_sigs`:** Re-borrows `ctx.impl_sigs` each iteration
(via `.get().and_then()` for the check, `.entry().or_default().insert()` for the
insert) to avoid holding `&mut` across the `ctx.error()` call — avoids a two-mutable-
borrow compile error without a local error buffer.

**`InferCtx` changes (`src/typechecker/env.rs`):**

`fn_sigs` widened from `HashMap<String, FnSig>` to `HashMap<String, (FnSig, Span)>`
to carry the first-definition span. `impl_sigs: HashMap<String, HashMap<String,
(FnSig, Span)>>` added. Both fields initialized in `InferCtx::new()`.

**New `TypeError` variants (`src/typechecker/error.rs`):**

- `DuplicateFn { name, first_span, duplicate_span }` — cites both sites; suggests rename
- `DuplicateMethod { type_name, method_name, first_span, duplicate_span }` — cites both
  sites, explains §22 merge rule ("all `impl {type_name}` blocks merge into one namespace"),
  suggests rename. Two distinct variants rather than one shared variant because the method
  error requires naming the type and explaining the merge rule — a `context: String` field
  would provide no actual simplification.

**`expr.rs` updates (`src/typechecker/infer/expr.rs`):**

Two `fn_sigs` access sites updated to destructure `(FnSig, Span)`:
- Line 29: `ctx.fn_sigs.get(*name)` → `if let Some((sig, _)) = ctx.fn_sigs.get(*name)`
- Line 177: `ctx.fn_sigs.get(name).cloned()` → `match ctx.fn_sigs.get(name) { Some((s, _)) => s.clone() }`

**Tests added (`src/typechecker/infer/mod.rs`):**

6 new tests: `error_duplicate_free_fn`, `error_duplicate_method_same_type`,
`ok_two_impl_blocks_non_overlapping`, `ok_duplicate_method_name_different_types`,
`ok_free_fn_and_method_same_name`, `error_duplicate_fn_and_method_coexist`.

**Agent reviews:**

`pillars-reviewer` — approved, no violations. Net pillar-1 improvement (closes
documented silent-overwrite gap). Pillar-5 satisfied: both new variants cite multiple
spans. One cosmetic note: `DuplicateFn` message shape (single em-dash line) differs
from `DuplicateMethod` (separate `note:`/`suggestion:` labels) — not a violation.
Pre-existing byte-offset rendering (`at byte {}`) is codebase-wide; not this PR's fix.

`rust-idiom-reviewer` — clean. No unsafe, no swallowed errors, no avoidable clones,
no unnecessary `Arc`/`Rc`. Re-borrow-per-iteration pattern confirmed sound. One finding
rejected: reviewer suggested `type_name_span` (impl block header) over `f.name_span`
(method name) in `DuplicateMethod`. Disagreement: for "duplicate method `bar`",
pointing at the conflicting method names is more actionable than pointing at the
enclosing `impl` header; the error already names the type.

**PR:** #26 (`feat/impl-merge-conflict-detection` → `main`, merged 2026-07-15, fast-forward).

**Test and lint state:** 164 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Resolved open items:**

- ✅ Pre-existing pillar-1 gap: `collect_fn_sig` silent overwrite on duplicate free-fn names — **closed by PR #26**.
- ✅ §22 whole-program impl-block merge + conflict detection — **closed by PR #26**.

**Multi-file note (documented, not a gap now):**

`main.rs` reads one `.ofn` file; "whole program" = one `Ast`. Cross-file duplicate
detection works when both `impl` blocks are in the same source file — the only case
that currently exists. Fix when multi-file pipeline lands.

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.

**Pending / next steps:**

- **Typechecker method/self resolution** — `bind_param` still defers `Type::SelfTy`;
  `Expr::MethodCall` / `Expr::Field` still deferred. Now unblocked (impl-block
  namespace exists). Next natural step.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-15 — §22 impl block parser + AST (PR #25)

**What was done:**

Added `Item::Impl` AST variant and parser support for `impl TypeName { ... }` blocks,
formalizing the block structure decided in §22 of `SYNTAX_SPEC.md`.

**AST changes (`src/ast/item.rs`, `src/ast/mod.rs`):**

- Added `ImplBlock<'src>` struct: `type_name: &'src str`, `type_name_span: Span`,
  `methods: Vec<FunctionDef<'src>>`, `span: Span`
- Added `Item::Impl(ImplBlock<'src>)` variant — old `// Struct, Enum, TypeAlias,
  ImplBlock — next PR` comment updated to reflect ImplBlock is now done
- `ImplBlock` exported from `ast::mod`

**Parser changes (`src/parser/item.rs`, `src/parser/mod.rs`):**

- `parse_impl_block`: eats `impl TypeName { fn* }`; reuses `parse_function` for each
  item inside; `Token::Eof` → "add `}` to close the impl block"; non-`fn` → hard parse
  error citing §22 via `error_expected`
- `parse_item` updated: `Token::Impl` arm added; top-level error now mentions both
  `fn` and `impl`
- `parse_impl` test helper added to `parser::mod`

**Typechecker changes (`src/typechecker/infer/mod.rs`):**

Both passes converted from `let Item::Function(f) = item else { continue }` to
exhaustive `match`:
```rust
match item {
    Item::Function(f) => collect_fn_sig(f, &mut ctx),
    Item::Impl(_) => {} // method type-checking deferred — future session
}
```
Exhaustive match ensures future `Item::Struct` / `Item::Enum` variants produce a
compile-time tripwire rather than silently being skipped. Test helper at line 196
updated to `let...else { panic!(...) }`.

**Agent reviews:**

`pillars-reviewer` — approved; no violations. Confirmed non-fn error cites §22 in
message and that `error_expected` routes the string through `Display` correctly.
Noted that the pre-existing `collect_fn_sig` `HashMap::insert` silent-overwrite gap
becomes a real pillar 1 issue once methods are collected — correctly deferred.

`rust-idiom-reviewer` — two findings acted on before commit:
- Finding A: `let...else { continue }` → exhaustive `match` in both typechecker
  passes (compile-time tripwire for future variants)
- Finding C: `_ =>` arm in `parse_impl_block` changed from hand-rolled
  `ParseError::UnexpectedToken` to `error_expected` helper (consistency)

Findings B (dead error path on final `eat` — by design) and D (tighter error-type
assertion in one test — minor) noted, not acted on.

**PR:** #25 (`feat/parse-impl-block` → `main`, merged 2026-07-15, fast-forward).

**Test and lint state:** 158 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Spec changes this session (`docs/SYNTAX_SPEC.md`):**

- Added §22 `impl` block syntax (Decided) — structure, multiplicity, conflict
  detection, pillar rationale, deferred note on `impl Trait for Type`
- Old §22 (deferred) renumbered to §23; all cross-references updated
- Pre-existing pillar 1 gap noted in §22 rationale: `collect_fn_sig` at
  `src/typechecker/infer/mod.rs:56` uses bare `HashMap::insert` → silently
  overwrites duplicate free-function names

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.
2. **Pre-existing pillar 1 gap:** `collect_fn_sig` (`src/typechecker/infer/mod.rs:56`)
   silently overwrites duplicate free-function names via `HashMap::insert`. Fix in the
   same session that builds impl-block conflict detection — they share a
   declaration-collection pass.

**Pending / next steps:**

- **Whole-program impl merge + conflict detection** — next natural step after this PR.
  Build a declaration-collection pass that merges all `Item::Impl` blocks for the same
  type and hard-errors on duplicate method names (citing file+line). Fix the
  `collect_fn_sig` overwrite gap in the same pass.
- **Typechecker method/self resolution** — `bind_param` still defers `Type::SelfTy` to
  `Ty::Error`; `Expr::MethodCall` / `Expr::Field` still deferred in `infer/expr.rs`.
  Unblocked once conflict detection is in place.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-14 — §18 self receiver enforcement (PR #24)

**What was done:**

Fixed three related bugs in `src/parser/item.rs:parse_params`:

1. **`&self`/`&mut self` accepted silently** — §18 states these forms "do not exist in
   Ofan source code" (pillar 3). Both now produce `ParseError::UnexpectedToken` with
   §18-citing suggestion to use bare `self` or `move self`.

2. **Bare `self` wrong type** — was `Type::Named { name: "Self" }`, so `bind_param`
   in the typechecker did not recognise it as a self receiver and emitted "user-defined
   type" in deferred diagnostics. Fixed to `Type::SelfTy(span)`.

3. **`move self` not parsed** — `Token::Move` existed since §17 but `parse_params`
   had no case for it. Now produces `Param { consuming: true, ty: Type::SelfTy }`.

**AST change:**

`Param` in `src/ast/item.rs` gains `pub consuming: bool`. False for all regular params
and bare `self`; true only for `move self`. Available for phase 2 method dispatch.

**Typechecker change:**

`bind_param` in `src/typechecker/infer/mod.rs` — `is_self_receiver` simplified from
two-arm (`Type::SelfTy` OR `Type::Ref { inner: SelfTy }`) to single arm
(`Type::SelfTy` only). The `Ref { inner: SelfTy }` arm is no longer reachable from
receiver position. Both `self` and `move self` still defer to `Ty::Error` in phase 1.

**Agent reviews:**

`pillars-reviewer` — approved; fix directly serves pillar 3 (eliminates second valid
receiver spelling) and pillar 1 (previously silent acceptance now hard error).
Minor note: `found` label imprecise when `&mut` not followed by `self` (e.g. `fn
f(&mut)`) — addressed before commit by tracking `has_self` in the consume loop.

`rust-idiom-reviewer` — approved after three fixes:
- Stale doc comment on `bind_param` ("Handles `&self`/`&mut self`") updated
- Comment added explaining why `Amp` branch hand-rolls `ParseError` rather than using `error_expected`
- Test asymmetry fixed: `parse_fn_ref_mut_self_is_error` now also asserts `infer`

**PR:** #24 (`fix/parse-params-self-receiver` → `main`, merged 2026-07-14, fast-forward).

**Test and lint state:** 151 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Resolved open items:**

- ✅ Open item #1 (carried from 2026-07-13): `parse_params` accepts `&self`/`&mut self`
  contradicting §18 — **closed by PR #24**.

**Known open items (carried forward):**

1. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.

**Pending / next steps:**

- **Typechecker phase 2: method/self resolution** — `parse_params` fix now complete;
  next is replacing `Deferred` for `self`/`Self` in `infer/convert.rs` and implementing
  `Expr::MethodCall` / `Expr::Field` resolution (currently deferred in `infer/expr.rs`).
- **Impl block syntax** — `Item::Function` is the only variant; `Item::Impl` needs
  parser + AST support before method resolution can land.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

## Last session: 2026-07-13 — AST modularization (PR #23)

**What was done:**

Reviewed entire codebase for modularization opportunities. Lexer, parser, and typechecker
were already well-modularized (products of PRs #6, #16, #19, #21). One genuine candidate:
`src/ast/mod.rs` — 283-line monolith with all AST node types in a single file.

Split `src/ast/mod.rs` into five focused submodules mirroring the parser's structure:

| File | Contents |
|------|----------|
| `ty.rs` | `Type<'src>`, `Type::span()`, `RefRegion<'src>` |
| `pattern.rs` | `Pattern<'src>`, `Pattern::span()` |
| `expr.rs` | `Expr<'src>`, `Expr::span()`, `MatchArm`, `BinOp`, `UnaryOp`, `BorrowKind` |
| `stmt.rs` | `Stmt<'src>` |
| `item.rs` | `Ast<'src>`, `Item<'src>`, `FunctionDef<'src>`, `Param<'src>` |

`Block` and `Literal` kept in `mod.rs`: referenced by multiple siblings; a dedicated
file for two small leaf types would be noise. All types re-exported from `mod.rs` —
zero call-site import changes in parser, typechecker, or `main.rs`.

Submodules use private `mod` (not `pub mod`), so only the `pub use` re-exports at
`mod.rs` are externally reachable — one canonical path per type (pillar 3 clean at the
Rust impl level too).

**Agent reviews:**

`rust-idiom-reviewer` — approved after one fix: merged split `use super::` lines into
single grouped imports in `expr.rs` and `stmt.rs`.

`pillars-reviewer` — approved; no violations. Note: reviewer flagged that the comment
about "avoiding cross-sibling imports" was slightly inaccurate (siblings already import
from parent freely via `use super::X`); the real reason is avoiding a dedicated module
for two small shared leaf types. Comment tightened before commit.

**PR:** #23 (`refactor/ast-split` → `main`, merged 2026-07-13, fast-forward).

**Test and lint state:** 147 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Known open items (carried forward):** unchanged — see PR #22 session below.

---

## Last session: 2026-07-13 — parser SelfKw fix (PR #22)

**What was done:**

Two tasks this session:

1. Verified §18 of `docs/SYNTAX_SPEC.md` against the settled self/Self design —
   confirmed it is already complete and accurate (written in the 2026-07-12 session).
   No spec changes needed.

2. Fixed the `Token::SelfKw` parser bug in `src/parser/types.rs` (open item #1 from
   2026-07-07, carried through the 2026-07-12 session).

**Investigation findings:**

The bug description was partially stale. Most of the fix was already in place:

- `Type::SelfTy(Span)` already existed in `ast/mod.rs`
- `parse_type` already handled capital `Self` correctly via the `Token::Ident("Self")`
  check in the `Ident` arm (line 24–26)
- `typechecker/infer/convert.rs` already pattern-matched on `Type::SelfTy` (deferred)

The two remaining bugs were:
1. `is_type_start` in `try_parse_region_tag` still included `Token::SelfKw` (lowercase
   `self` is never a valid type; capital `Self` lexes as `Token::Ident("Self")` and was
   already covered by `Token::Ident(_)`)
2. `parse_type` had no `Token::SelfKw` arm — fell to the generic "expected a type"
   catch-all with no guidance about `Self` (capital)

**Changes (src/parser/types.rs only):**

- Extracted `is_type_start_token` predicate — single source of truth for "which tokens
  can start a valid type"; used by `try_parse_region_tag` in place of the inline `matches!`
  that previously included `Token::SelfKw` (rust-idiom-reviewer suggestion to prevent
  future drift between the heuristic and `parse_type`'s own dispatch)
- Removed `Token::SelfKw` from the region-tag heuristic
- Added `Token::SelfKw` arm in `parse_type` with a pillar-5 error: names the problem,
  points at the site, suggests `Self` (capital), cites §18
- Added 4 tests: `parse_type_self_ty`, `parse_type_ref_self_ty`,
  `parse_type_region_ref_self_ty`, `parse_type_self_kw_in_type_position_is_error`
  (error-path assertion guards the suggestion text — "Self", "receiver", "§18")

**Agent reviews:**

`pillars-reviewer` — approved; no violations. Fix strengthens pillars 1, 3, 5.
Confirmed `&self`/`&mut self` in `parse_params` (item.rs) is a real §18 pillar-3
discrepancy (those forms "do not exist in Ofan source code" per §18) — tracked
separately (see open items below).

`rust-idiom-reviewer` — approved after one design note acted on:
extracted `is_type_start_token` to prevent drift between the heuristic and dispatch.
One future calcification point noted but not acted on: `ParseError`'s suggestion
field is `String` (stringly-typed content); if machine-readable diagnostics are ever
needed, `suggestion` should become a structured variant. Not required now.

**PR:** #22 (`fix/parser-selfkw-type-start` → `main`, merged 2026-07-13, fast-forward).

**Test and lint state:** 147 passed, 0 failed. `cargo clippy -- -D warnings` clean.

**Resolved open items:**

- ✅ Open item #1 (2026-07-07): `try_parse_region_tag` `Token::SelfKw` lookahead
  inconsistency — **closed by PR #22**.

**Known open items (carried forward):**

1. **`parse_params` contradicts §18** — `src/parser/item.rs:65-92` currently parses
   `&self` and `&mut self` as receiver forms; §18 (SYNTAX_SPEC.md:951–954) explicitly
   states "These forms do not exist in Ofan source code." Additionally, `self` params
   get `ty: Type::Named { name: "Self" }` rather than proper access-mode inference.
   This is the `bind_param`/typechecker self-binding work: replacing `Deferred` for
   `self`/`Self` params in `infer/convert.rs`, and removing the `&self`/`&mut self`
   forms from `parse_params`.
2. Pre-existing `cargo clippy --all-targets` issues in `numbers.rs` test code — not
   in the lint gate.

**Pending / next steps:**

- **Typechecker phase 2: method/self resolution** — `parse_params` fix (above) is a
  prerequisite; once `self` binds correctly in the typechecker, `Expr::MethodCall` and
  `Expr::Field` can be undeferred.
- **Typechecker phase 2: lifetime/region inference + Copy/Move enforcement** — partially
  blocked until method/self resolution is in place.
- **`docs/ARCHITECTURE.md`** — high-level compiler-phase map.
- **Anchor CLI tool** — real program to compile; validates language design against usage.

---

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
