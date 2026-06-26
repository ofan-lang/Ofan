# Ofan — Language Syntax Reference

> **What this file is:** The canonical home for Ofan's concrete syntax — keyword shapes,
> operator forms, literal rules, and token-level decisions. For the *why* behind the
> pillars that motivate these decisions, see [`docs/PHILOSOPHY.md`](PHILOSOPHY.md). For
> compiler implementation details, see `docs/ARCHITECTURE.md` (not yet created).

> **Status legend:**
> - **Decided** — locked, unless a future design session explicitly reopens it with a
>   documented rationale
> - **UNRESOLVED** — open question; do not implement until resolved in a follow-up session
> - **TENTATIVE** — provisional direction, not fully locked; treat as subject to change

---

## Contents

| § | Topic | Status |
|---|-------|--------|
| [1](#1-comments) | Comments | UNRESOLVED |
| [2](#2-identifiers--casing) | Identifiers & casing | UNRESOLVED |
| [3](#3-statement-termination) | Statement termination | Decided |
| [4](#4-block-delimiters) | Block delimiters | Decided |
| [5](#5-variable-declarations) | Variable declarations | Decided |
| [6](#6-function-declarations) | Function declarations | Decided |
| [7](#7-lifetime--region-inference-and-escape-hatch) | Lifetime / region inference & escape hatch | Decided (core) — UNRESOLVED sub-item |
| [8](#8-casting) | Casting | Decided |
| [9](#9-type-annotation--—-resolved-overload) | Type annotation `:` — resolved overload | Decided |
| [10](#10-struct-literal-fields) | Struct literal fields | Decided |
| [11](#11-type-aliasing) | Type aliasing | Decided |
| [12](#12-operators) | Operators | Decided |
| [13](#13-pointers-and-raw-memory) | Pointers and raw memory | TENTATIVE — UNRESOLVED sub-item |
| [14](#14-not-yet-decided--deferred) | Not yet decided — deferred | — |

---

## Status summary

At a glance: **9 decided**, **2 unresolved**, **1 tentative with an unresolved sub-item**,
**1 decided-with-an-unresolved-sub-item**, **1 deferred-items list**.

This spec is not yet complete enough to fully drive lexer implementation — §1, §2, and the
sub-items in §7 and §13 must be resolved first. See §14 for everything not yet started at
all.

---

## §1 Comments

**UNRESOLVED.** Not decided in any session. Open questions: single-line (`//`), block
(`/* */`), doc comments, or some combination. Needs an explicit decision before the lexer
can handle comments. Do not assume a form and implement it.

---

## §2 Identifiers & casing

**UNRESOLVED.** Not decided. Open question: is casing (e.g. `snake_case` for variables,
`PascalCase` for types) enforced by the compiler as an error, enforced as a warning, or
merely conventional?

Pillar 3 implications either way: compiler-enforced casing produces one canonical form in
shared source; convention-only allows multiple valid forms to coexist. Needs an explicit
decision before identifier normalization can be defined.

---

## §3 Statement termination

**Decided: required semicolons.**

```ofn
let x: i32 = 5;
return x;
```

*Alternatives rejected:* newline-significant termination and ASI (automatic semicolon
insertion).

*Rationale (pillars 1, 3, 5):* A missing semicolon is a precise, explicit, easily-
suggestible parse error — a good fit for pillar 5. Newline-significance and ASI both
introduce a second, implicit grammar governing where statements end, in tension with pillar 3
(single canonical syntax) and pillar 1 (explicit, never silent erroneous behavior).

---

## §4 Block delimiters

**Decided: braces `{ }`, mandatory, never optional.**

```ofn
if cond { return x; }    // single-statement — braces still required

if cond {                // multi-line
    return x;
}
```

*Alternatives rejected:*
- **Significant whitespace:** tooling disagreement about what is canonical (tabs vs. spaces,
  continuation rules) violates pillar 3.
- **Keyword blocks (`do...end`):** slower to type, no structural advantage.
- **Optional braces for single-statement bodies:** creates two valid ways to write the same
  construct (violates pillar 3). Also a historically documented source of silent structural
  bugs — the "goto fail" class of bug is a direct instance of the same omission. Pillar 1
  prohibits constructs that allow silent structural ambiguity.

*One-line brevity:* still achievable by placing the brace pair on one line
(`if cond { return x; }`). The restriction costs no expressiveness, only discards the
ambiguous form.

---

## §5 Variable declarations

**Decided.**

```ofn
let x: i32 = 5;          // immutable binding
let mut x: i32 = 5;      // mutable, via modifier
const MAX: i32 = 100;    // compile-time constant
let x = 5;               // type annotation optional when inferable
```

*Keywords:* `let` and `const` only. No third `var` keyword.

*Mutability:* `mut` is a modifier on `let`, not a separate keyword. This gives one mechanism
that applies consistently (bindings, parameters, references) rather than three keywords
teaching overlapping concepts. Consistent with pillar 3 (one canonical form) and pillar 2
ergonomics (one rule to learn).

*Type annotation:* optional when the type is inferable, consistent with how lifetime
inference is handled elsewhere — annotation appears only when there is something to annotate
that cannot be derived automatically.

---

## §6 Function declarations

**Decided.**

```ofn
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn log_error(msg: &str) {
    print(msg);
}
```

- `fn` keyword (not `func` or `def`).
- `->` for return type annotation. `:` was tried and reverted — see [§9](#9-type-annotation--—-resolved-overload) for the full history.
- No trailing `->` when there is no return value. Mirrors the `let` type-annotation rule:
  annotation appears only when there is something to annotate.

---

## §7 Lifetime / region inference and escape hatch

**Decided (core mechanism).** One sub-item remains UNRESOLVED — see below.

Inference is the default and covers the common case (single input/output borrow relationship)
with zero annotation. The escape hatch is a **plain lowercase word acting as a region tag**,
attached to `&`, used only when the compiler cannot disambiguate which borrows are linked:

```ofn
fn first_word(s: &str) -> &str { ... }                  // pure inference, no tag needed

fn app_name() -> &static str { "Ofan Compiler" }         // &static: keyword, not a tag —
                                                           // no linking needed

fn longest(a: &r1 str, b: &r1 str) -> &r1 str { ... }   // tag links three positions

fn pick_first(a: &r1 str, b: &r2 str) -> &r1 str { a }  // distinct tags for
                                                           // unrelated lifetimes
struct Parser<r1> {
    source: &r1 str,
    pos: usize,
}
```

*Rationale (pillar 2):* Rust's `'a` sigil syntax is exactly the symbol-soup the
"lower learning curve" thesis is trying to avoid, given that annotations should be rare
under inference. A bare word reads as an ordinary identifier rather than introducing a new
sigil-namespace. `&static` is a keyword (intent: "lives for the whole program") and does
not require a tag, since it isn't linking two positions — only cases that genuinely require
linking distinct borrows need a named tag.

**UNRESOLVED sub-item:** `<...>` is used both for region tags (`Parser<r1>`) and will be
needed for type generics (`Stack<T>`). This is tentatively accepted as "the same category
of thing" (compile-time parameters) rather than a true ambiguity, but not formally closed
out. Pillar 3 (single canonical syntax) is not fully satisfied for this construct until it
is. Do not assume this is settled.

> **See also:** [§13](#13-pointers-and-raw-memory) — if raw pointers are eventually given
> their own lifetime tracking, this sub-item and the `unsafe` question there may need to be
> resolved together rather than independently.

---

## §8 Casting

**Decided: `as` keyword.**

```ofn
let mean = sum / (values.len() as f64);
```

*Alternative rejected:* `cast<Type>(value)`. This would reuse `<...>` for a third purpose
(alongside generics and region tags), give up `as`'s left-to-right English-readable order
("this value, as this type"), and add no clear benefit. `as` is one keyword, unambiguous,
and has prior-art familiarity with no learning-curve cost.

---

## §9 Type annotation `:` — resolved overload

**Decided:** `:` is reserved **exclusively** for "a type follows." This applies consistently
to variable declarations and parameter declarations.

*Preserved history:* `:` was briefly tried for return types too (`fn f(): i32`), to avoid
introducing `->`. Rejected after review: stacking three `:` jobs onto one line
(`fn add(a: i32, b: i32): i32`) created a scannability problem. The reader cannot visually
distinguish "param type" colons from "return type" colons without parsing context — even
though each individual use was unambiguous in isolation. `->` was reinstated because a
structurally distinct glyph lets the reader jump directly to the return type without
scanning through parameter colons first.

**Key insight for future symbol-reuse proposals:** this was a density/scannability problem,
not a pure keystroke problem. Symbol reuse that is individually unambiguous can still
degrade readability at the line level.

---

## §10 Struct literal fields

**Decided: `=` for field values, not `:`.**

```ofn
Ok(Stats { mean = mean, min = min, max = max })
```

*Rationale (pillar 3):* `:` in `let x: i32` means "a type follows." Using `:` in
`Stats { mean: mean }` would mean "a value follows" — two different relationships on one
symbol. `=` already means "a value is being bound" everywhere else (`let x = 5`). Using `=`
here is consistency, not novelty, and closes a genuine ambiguity rather than a cosmetic one.

---

## §11 Type aliasing

**Decided: `using <Type> as <alias>;`**

```ofn
using f64 as f;
```

*Rationale (pillar 3 note):* this aliases a *name*, not a *syntax form*, so it does not
conflict with pillar 3, which governs syntax-level aliasing (e.g. permitting both `fn` and
`func` in source — which Ofan does not do). User-defined naming choices are an ordinary
language feature, comparable to Rust's `type` alias.

---

## §12 Operators

**Decided — full confirmed set.**

### Arithmetic

| Operator | Meaning |
|----------|---------|
| `+` | addition |
| `-` | subtraction / unary negation |
| `*` | multiplication |
| `/` | division |
| `%` | modulo |
| `+=` `-=` `*=` `/=` `%=` | compound assignment |

### Comparison

| Operator | Meaning |
|----------|---------|
| `==` | equality |
| `!=` | inequality |
| `<` `>` `<=` `>=` | ordered comparison |

### Logical

| Operator | Meaning |
|----------|---------|
| `&&` | logical and (short-circuit) |
| `\|\|` | logical or (short-circuit) |
| `!` | logical not |

### Bitwise

| Operator | Meaning |
|----------|---------|
| `&` | bitwise and |
| `\|` | bitwise or |
| `^` | bitwise xor |
| `~` | bitwise not |
| `<<` `>>` | left / right shift |

*Rationale for keeping standard forms:* the full operator surface is near-monocharacter;
no further compression was pursued. Two-character operators (`+=`, `==`, `&&`, `||`, etc.)
each follow one of two consistent rules (doubling = logical form of the bitwise op; trailing
`=` = comparison or compound assignment). Further compression would break the learnable
pattern and collide with the primitive each is built from.

### Error propagation — `?`

```ofn
fn read_config() -> Checked<Config, &str> {
    let raw = read_file("config.ofn")?;
    let parsed = parse(raw)?;
    Ok(parsed)
}
```

Applies to `Checked<T, E>` or `Option<T>`. On failure/absence, immediately returns from the
enclosing function with the error. On success/presence, evaluates to the unwrapped value.

*Rationale (pillar 1):* a visible call-site marker of a fallible operation — the failure
path is marked, not invisible, unlike C++ exceptions.

### Fallback — `?:`

```ofn
let timeout = config.timeout ?: 30;
let name = user.nickname ?: user.full_name ?: "anonymous";
```

Applies to `Option<T>` **only**. Explicitly **not** valid on `Checked<T, E>`.

*Behavior:* pure expression, no control-flow change. Left-associative (`(a ?: b) ?: c`),
short-circuits left-to-right (right operand only evaluated if left is absent), consistent
with `&&` / `||`.

*Why `Checked<T, E>` is excluded (pillar 1):* allowing a fallback value to silently discard
an `Err` would be exactly the "silent erroneous behavior" pillar 1 prohibits. Discarding an
error must never be the cheap/fast path — `match` remains the only way to supply a fallback
for a `Checked` value, by design.

*C-style ternary `cond ? a : b` rejected:* would overload both `?` (already "propagate")
and `:` (already "type follows") with a third, positionally-dependent meaning —
reintroducing the exact class of symbol-overload problem that [§9](#9-type-annotation--—-resolved-overload) and [§10](#10-struct-literal-fields) above were designed
to prevent.

---

## §13 Pointers and raw memory

**TENTATIVE — treat as provisional, not fully locked.**

```ofn
let x: &i32 = &y;               // safe borrow (decided, see §7)
let p: *i32 = raw_ptr();        // raw/unsafe pointer — distinct glyph
let b: Box<i32> = Box::new(5);  // owning heap pointer — ordinary generic type
```

`*` is proposed to be reserved **only** for raw pointers — never reused for multiplication
in a position-dependent way. This avoids the known C readability wart where `*` means
"pointer type" in a declaration and "dereference" in an expression.

**UNRESOLVED:** whether raw-pointer code requires a marked `unsafe { }` block (as in Rust).
This is used provisionally in design examples but not formally decided. Consequently,
`unsafe` is **not** a confirmed reserved keyword — do not add it to the keyword table until
this is resolved.

> **See also:** [§7](#7-lifetime--region-inference-and-escape-hatch) — the `<...>` overload
> question there may interact with raw pointers if they ever need region tracking.

---

## §14 Not yet decided — deferred

The following constructs have appeared informally in design examples but have **never been
formally decided**. Do not assume any particular syntax is settled for these:

- **Loop syntax** — `for`/`while`/`loop` used informally; exact forms and semantics undecided
- **`match` / pattern matching** — used informally in examples; syntax undecided
- **`Option<T>` / `Checked<T, E>` variant names** — `Some`/`None`, `Ok`/`Err` equivalents
  not finalized; `Checked` as a pillar-1-flavored rename of `Result` was discussed, not
  finalized
- **Method receiver syntax** — `self` vs `&self` vs `mut self` raised, not resolved
- **Copy vs. Move semantics** — raised, not resolved; directly affects receiver syntax
- **Trait / interface syntax** — not started
- **Module / import syntax and path separator** — `::` used informally in examples only
- **Enum declaration syntax** — not decided
- **`#[no_runtime]`-style attributes** — appeared in one exploratory example only

These do not block lexer work on the tokens that *are* decided above, but the token set
will need a follow-up pass once they are resolved.

---

*Source: content migrated from `docs/prds/2026-06-26-lexer.md` during the 2026-06-26
documentation reorganization.*