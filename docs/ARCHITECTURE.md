# Architecture — Ofan compiler

The Ofan compiler is implemented in Rust (`src/`). The frontend is complete through
typechecking (lexer, parser, typechecker phase 1); codegen exists as a skeleton only.
For language syntax decisions see `docs/SYNTAX_SPEC.md`; for design pillars and
semantic rationale see `docs/PHILOSOPHY.md`; for session history and what was decided
when see `docs/PROGRESS.md`.

---

## Compilation pipeline

```
source text
    │
    ▼  src/lexer/
 Lexer::new(src).lex()
    │  Vec<(Token<'src>, Span)>
    ▼  src/parser/
 Parser::new(tokens).parse()
    │  Ast<'src>
    ▼  src/typechecker/
 typechecker::infer(&ast)
    │  InferResult  (deferred warnings → stderr)
    ▼  src/codegen/
 ⚠ NOT YET IMPLEMENTED
    (prints "codegen not yet implemented", exits 1)
```

See `src/main.rs` for the wiring.

---

## Lexer  (`src/lexer/`)

**Entry point:** `lexer::Lexer::new(src: &str).lex()`
**Output:** `Result<Vec<(Token<'src>, Span)>, LexError>`

**Key types:**
- `Token<'src>` — zero-copy; string-payload variants hold `&'src str` slices into the source
- `Span { start: usize, end: usize }` — byte offsets; used as the identity key for nodes throughout the pipeline
- `LexError` — always fatal; one error per invocation (first problem stops the scan)

**Submodules:**

| File | Responsibility |
|------|----------------|
| `mod.rs` | `Lexer` struct, `lex()` entry point, thin char dispatcher |
| `token.rs` | `Token<'src>` enum (all token variants + `Span`) |
| `error.rs` | `LexError` variants |
| `keywords.rs` | keyword lookup table (identifier → keyword token) |
| `numbers.rs` | decimal / float / hex / binary / octal scanning |
| `strings.rs` | string literal scanning + escape validation |
| `chars.rs` | character literal scanning |
| `comments.rs` | line (`#`), block (`##…##`), doc (`###…###`) comments |
| `operators.rs` | multi-character operator scanning |
| `punctuation.rs` | single-character punctuation dispatch table |
| `escapes.rs` | shared `decode_escape` helper (used by strings + chars) |

**Completeness:** complete for all §1–§15 constructs. One pre-existing non-blocking
`cargo clippy --all-targets` note in `numbers.rs` (not in the lint gate).

---

## AST  (`src/ast/`)

The AST is a shared data structure, not a pipeline phase. It is produced by the parser
and consumed by the typechecker (and, eventually, codegen).

**Design:** zero-copy — all string-like nodes hold `&'src str` slices. Every node
carries a `Span` so the typechecker can key its `type_map` and error messages on
source locations.

**Key types:**
`Ast<'src>` (top-level item list), `Item<'src>` (Function / Impl / Struct),
`FunctionDef`, `ImplBlock`, `StructDef`, `Param`, `Block`, `Expr`, `Stmt`,
`Type`, `Pattern`, `CopyMove`

**Submodules** (all private `mod`; re-exported from `mod.rs`):

| File | Key exports |
|------|-------------|
| `mod.rs` | `Block`, `Literal`, all re-exports |
| `item.rs` | `Ast`, `Item`, `FunctionDef`, `ImplBlock`, `Param`, `StructDef`, `StructField`, `CopyMove` |
| `expr.rs` | `Expr`, `MatchArm`, `BinOp`, `UnaryOp`, `BorrowKind` |
| `stmt.rs` | `Stmt` |
| `ty.rs` | `Type`, `RefRegion` |
| `pattern.rs` | `Pattern` |

`Block` and `Literal` live in `mod.rs` directly — both are needed by multiple siblings
and a dedicated sixth file for two small leaf types would be noise.

---

## Parser  (`src/parser/`)

**Entry point:** `parser::Parser::new(tokens: Vec<(Token<'src>, Span)>).parse()`
**Output:** `Result<Ast<'src>, ParseError>`

**Key types:**
- `Parser<'src>` — recursive-descent, single-pass, no backtracking
- `ParseError { message, suggestion: String, span }` — `suggestion` is always
  populated (pillar 5: every error includes context + a fix suggestion)

**Submodules:**

| File | Responsibility |
|------|----------------|
| `mod.rs` | `Parser` struct, `parse()` entry, `parse_block`, test helpers |
| `error.rs` | `ParseError`, `error_expected` helper |
| `item.rs` | `parse_function`, `parse_impl_block`, `parse_struct_def` |
| `types.rs` | `parse_type`, `is_type_start_token`, `try_parse_region_tag` |
| `stmt.rs` | `parse_stmt` |
| `expr.rs` | expression grammar (precedence climbing) |
| `control_flow.rs` | if / while / loop / for / match |
| `pattern.rs` | pattern grammar |

**Completeness:** complete for all currently specified constructs (§1–§23).
Match and for-in bodies are parsed; their semantic typechecking is deferred
(see Typechecker § deferred below).

---

## Typechecker  (`src/typechecker/`)

**Entry point:** `typechecker::infer(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>>`

**Key public types:**
- `InferResult` — opaque result; `type_map: HashMap<Span, Ty>` (query via `type_of(span)`),
  `deferred: Vec<TypeError>` (non-fatal; codegen must not lower `Ty::Error`-typed nodes)
- `Ty` — resolved type (not the same as `ast::Type`; names are looked up, primitives collapsed)
- `FnSig { params, return_ty, is_generic, self_consuming }`
- `TypeError` — via `thiserror`; all fatal variants carry `suggestion` (pillar 5)

### Inference state  (`env.rs`)

`InferCtx` carries:

| Field | Type | Purpose |
|-------|------|---------|
| `fn_sigs` | `HashMap<String, (FnSig, Span)>` | free-function signatures |
| `impl_sigs` | `HashMap<String, HashMap<String, (FnSig, Span)>>` | method sigs by type name |
| `struct_defs` | `HashMap<String, StructInfo>` | struct field tables |
| `type_map` | `HashMap<Span, Ty>` | expression span → resolved type |
| `errors` | `Vec<TypeError>` | accumulated fatal errors |

`Env` is a separate scope stack for variable bindings (`push_scope / pop_scope / define / lookup`).

### Two-pass orchestration  (`infer/mod.rs`)

**Pass 1** — three sub-passes, all complete before any body is type-checked:

| Sub-pass | Function | What it does |
|----------|----------|--------------|
| 1a | `collect_struct_name` | Register struct name + span; `DuplicateStruct` on collision |
| 1b | `collect_struct_fields` | Populate `StructInfo.fields` and `field_order`; 1a must precede so field types can forward-reference other structs |
| 1c | `collect_fn_sig` / `collect_impl_sigs` | Collect fn/method signatures; `DuplicateFn`, `DuplicateMethod` detection |

**Pass 2** — `infer_fn` / `infer_method` for each function/method body.
After `infer_block` returns, both call `check_tail_field_own_non_copy(f.body.tail)` to
catch implicit returns: `f.body.tail` is an `Expr`, not a `Stmt`, so `Stmt::Return` does
not reach it.

### Submodules

| File | Responsibility |
|------|----------------|
| `mod.rs` | Orchestration, `is_copy`, `check_field_own_non_copy`, `check_tail_field_own_non_copy`, `named_base_deref`, all tests |
| `expr.rs` | `infer_expr`, `infer_call`, `infer_method_call`, `infer_field_access` |
| `stmt.rs` | `infer_stmt` (Let / Return / Assign / Expr statements) |
| `ops.rs` | `infer_unary`, `infer_binary` (full operator type tables) |
| `convert.rs` | `ast_ty_to_ty`, `ast_region_to_region` |

### Phase 1 — what is checked

- Primitive types (`i32`, `f64`, `bool`, `char`, `str`, `()`)
- Literals, identifier resolution
- `let` / `const` bindings with optional annotation, `return`, simple assignment
- All unary and binary operators
- `if` / `while` / `loop`
- Free function calls (monomorphic): arg-count + per-arg type checking
- Method calls: self receiver inference (§18), `ConsumeViaRef` guard, `MethodNotFound` with available-method list
- Field reads: `FieldNotFound` with available-field list, cascade suppression when receiver is `Ty::Error`
- Field writes: `FieldWriteViaSharedRef` for `&T` receivers; `&mut T` and owned receivers pass through
- Partial-move detection: `FieldOwnNonCopy` at `let` init, `return`, and call-arg positions, including through tail-position wrappers (see Design patterns §3)
- Struct Copy/Move inference: `is_copy` — recursive over fields; `copy_override` wins
- Whole-program conflict detection: `DuplicateFn`, `DuplicateMethod`, `DuplicateStruct`

### Phase 1 — deferred (non-fatal)

These constructs are accepted with a `TypeError::Deferred` diagnostic and typed as
`Ty::Error` so inference can continue past them:

- Generic function/method call instantiation
- Generic struct field access
- `for` / `for-in` loops
- `match` arm typechecking
- Cast (`as`), `?` operator
- Compound assignment type checking

### Phase 2 — not started

- Lifetime / region inference (`Ty::Ref.region` is `None` throughout phase 1)
- Move tracking, `UseAfterMove`
- Borrow checking, `BorrowConflict`

Placeholder variants already exist in `TypeError` and `Ty` for API stability.
`PHASE2:` comments in `env.rs` and `ty.rs` mark the extension points.

---

## Codegen  (`src/codegen/`)

**Status: SKELETON ONLY.**

`src/codegen/mod.rs` contains `LlvmContext` wrapping `inkwell::context::Context` — no
lowering logic, no IR passes. `src/codegen/llvm.rs` is gated behind
`#[cfg(feature = "codegen")]` and requires LLVM dev libraries at build time; it is
excluded from default `cargo build`.

`main.rs` prints `"ofan: codegen not yet implemented"` and exits 1 after a successful
typechecking pass.

**Planned backend:** LLVM via inkwell (decided 2026-06-24; rationale in `PROGRESS.md` —
multi-platform reach without per-arch codegen; Cranelift evaluated and rejected).

---

## Cross-cutting design patterns

### 1. Inference-with-explicit-override

Three language features share the same three-case structure:

| Feature | Inferred | Explicit Copy override | Explicit Move override |
|---------|----------|------------------------|------------------------|
| §17 Copy/Move | struct is Copy iff all fields are Copy | `copy struct` | `move struct` |
| §18 self receivers | access mode inferred from body usage | — | `move self` (consuming) |
| §23 field access | Copy-eligibility via `is_copy()` follows §17 | — | — |

`is_copy()` in `src/typechecker/infer/mod.rs` is the shared implementation for §17 and §23.
When designing new features, ask: is there an "infer from context, explicit override to escape" form?

### 2. Whole-program declaration-collection pass

Pass 1 collects **all** names before pass 2 checks **any** body. This enables mutual
recursion between functions and forward references between struct field types. All
conflict detection lives in pass 1 sub-passes.

Future additions (enum declarations, trait impls, module namespaces) will extend pass 1
with additional `collect_*` sub-passes. The pattern is: add a sub-pass to 1a/1b/1c,
keep the body-checking pass 2 untouched.

### 3. Hard error over silent gap (Pillar 1) — tail-position transparency

When a new ownership or consumption check is added at a specific expression position
(let-init, return, call-arg, …), the check must also fire when the target expression
is wrapped in a **transparent tail-position construct**:

- `Expr::Block` — block tail (the `block.tail` field, no trailing `;`)
- `Expr::If` / `else` branches
- (future) `Expr::Match` arms — flagged with `NB` comment at the `_ => false` wildcard
  arm of `check_tail_field_own_non_copy`

This "surface check, silent bypass via one level of wrapping" class of bug occurred twice:

- **PR #27 Gap A / `ConsumeViaRef`:** consuming method call through a reference receiver
  was not checked when the receiver reached the call site through an intermediate position.
- **PR #28 `FieldOwnNonCopy`:** the check was wired to `Stmt::Let`, `Stmt::Return`, and
  call-arg positions — but `Expr::Block` and `Expr::If` wrappers bypassed it. The implicit
  function-body tail (`f.body.tail`) was missed entirely because it is an `Expr`, not a `Stmt`.

The fix: `check_tail_field_own_non_copy` (`src/typechecker/infer/mod.rs`) recurses into
`Expr::Block` tails and `Expr::If`/`else` branches before calling `check_field_own_non_copy`.
The pillars-reviewer agent (`.claude/agents/pillars-reviewer.md`) has a standing checklist
item for this pattern.

**Quick test:** if wrapping the flagged expression in `{ … }` or `if true { … } else { … }`
makes the check silent, it is a pillar-1 violation.

---

## Not yet designed

See `docs/SYNTAX_SPEC.md` §24 for the canonical deferred list. Short summary:

- Struct literal construction (`Point { x: 1.0, y: 2.0 }`) — parser not yet written
- Enum typechecking — AST + parser complete; typechecker deferred
- Traits / trait bounds
- Modules / namespaces (`mod`, `use`)
- Generic instantiation (phase 2 typechecker)
- Lifetime annotations (opt-in escape hatch per pillar 2)
- Standard library / prelude (`Option<T>`, `Checked<T, E>` constructors)
- C interop (explicit `extern` blocks — call-into-C only; decided in `PHILOSOPHY.md`)
- `for` / `for-in`, `match`, cast (`as`), `?` operator (parser complete; typechecker deferred)
- Codegen (any of it)

---

## Where to look

| Want to… | Look here |
|----------|-----------|
| Language syntax decisions | `docs/SYNTAX_SPEC.md` |
| Design pillars + rationale | `docs/PHILOSOPHY.md` |
| Session history + what was decided | `docs/PROGRESS.md` |
| Process, commit workflow, agent use | `CLAUDE.md`, `.claude/agents/` |
| Change how a token is scanned | `src/lexer/<construct>.rs` |
| Add a new AST node | `src/ast/<appropriate-submodule>.rs` |
| Add a new syntax construct | `src/parser/<appropriate-submodule>.rs` |
| Add a new `TypeError` variant | `src/typechecker/error.rs` |
| Add a type inference rule | `src/typechecker/infer/expr.rs` or `stmt.rs` |
| Add a pass-1 declaration check | `src/typechecker/infer/mod.rs` (`collect_*` fns) |
| Check Copy-eligibility for a type | `is_copy()` in `src/typechecker/infer/mod.rs` |
| Understand the tail-position transparency pattern | `check_tail_field_own_non_copy` in `src/typechecker/infer/mod.rs` |
