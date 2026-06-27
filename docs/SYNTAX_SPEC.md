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
| [1](#1-comments) | Comments | Decided |
| [2](#2-identifiers--casing) | Identifiers & casing | Decided |
| [3](#3-statement-termination) | Statement termination | Decided |
| [4](#4-block-delimiters) | Block delimiters | Decided |
| [5](#5-variable-declarations) | Variable declarations | Decided |
| [6](#6-function-declarations) | Function declarations | Decided |
| [7](#7-lifetime--region-inference-and-escape-hatch) | Lifetime / region inference & escape hatch | Decided |
| [8](#8-casting) | Casting | Decided |
| [9](#9-type-annotation--—-resolved-overload) | Type annotation `:` — resolved overload | Decided |
| [10](#10-struct-literal-fields) | Struct literal fields | Decided |
| [11](#11-type-aliasing) | Type aliasing | Decided |
| [12](#12-operators) | Operators | Decided |
| [13](#13-pointers-and-raw-memory) | Pointers and raw memory | Decided |
| [14](#14-numeric-literals) | Numeric literals | Decided |
| [15](#15-string-and-character-literals) | String and character literals | Decided (core) — 2 items deferred |
| [16](#16-not-yet-decided--deferred) | Not yet decided — deferred | — |

---

## Status summary

At a glance: **15 of 15 numbered sections decided** (§15 has two narrow, deliberately
deferred extensions — Unicode escapes and raw strings — that do not block the core
lexer work). Every token-level construct needed for a first lexer implementation is now
covered. The remaining open ground is §16 — constructs that have never been formally
designed at all (loops, `match`, method receivers, Copy/Move, traits, modules, enums,
attributes, array literals, generic call syntax, void/unit type) — which were always
out of scope for the lexer's first pass and do not block it.

---

## §1 Comments

**Decided: `#` line, `##...##` block, `###...###` doc — one symbol family, arity
determines meaning.**

```ofn
# regular single-line comment

##
regular block comment,
can span multiple lines
##

### doc comment — attaches to the item immediately below it
fn add(a: i32, b: i32) -> i32 { a + b }

###
A doc comment that spans multiple lines, closed by a matching triple-hash.
###
struct Stats { mean: f64 }
```

*Alternative considered:* standard `//` line / `/* */` block. Rejected in favor of the
`#`-based scheme below — not primarily for the 1-keystroke saving on `#` vs `//` (a real
but small win, same order of magnitude as the `->`/`:` decision in §9), but because
`//`/`/* */` are two unrelated symbol families for the same underlying concept ("ignore
this text"), whereas a single symbol family with arity-based meaning (`#`, `##`, `###`)
is more consistent with pillar 3 and mirrors how Ofan already disambiguates `&`/`&&` and
`|`/`||` elsewhere in §12 — one character, lookahead determines meaning.

*Lexer disambiguation rule:* on encountering `#`, peek ahead:
- if the next two characters are also `##` (three consecutive `#` total) → doc comment;
  scan until the next occurrence of `###` (no nesting)
- else if the next character is `#` (two consecutive `#` total) → block comment; scan
  until the next occurrence of `##` (no nesting)
- else → line comment; scan to end of line

This is the same one-character/two-character lookahead technique already used for
`&`/`&&`, `|`/`||`, and the compound-assignment operators in §12 — no new lexer technique
required, just applied to a new symbol.

*Doc-comment attachment:* attaches to the item **immediately following** it only
(forward-attaching). No "trailing"/"attaches above" form for v1 — matches the dominant
convention (Rust's `///`, Javadoc, Python's docstring-as-first-statement all attach
forward) and avoids solving a problem (documenting the *enclosing* item from inside it,
like Rust's `//!`) that hasn't come up yet. Can be added later if a real need arises.

*Practical ceiling:* three tiers (`#`/`##`/`###`) is the limit for this pattern — a
hypothetical fourth tier (`####`) would be hard to distinguish from `###` at a glance, but
no fourth tier is currently needed, so this is not a live constraint.

---

## §2 Identifiers & casing

**Decided: no compiler-enforced casing rule. Structural identifier grammar only.**

An identifier is a contiguous run of letters, digits, and underscores; it must not start
with a digit; it cannot contain whitespace or any reserved operator/punctuation character.
This is the same near-universal structural constraint every lexer requires (the reason
`My variable`, `var&&var`, `var#var`, and `Hello/world` are invalid is that each would
either split into multiple tokens or collide with an operator — not a casing issue at all).

Beyond that structural minimum, **casing style is unconstrained.** `my_var1`, `MyVar1`, and
`extremely_long_variable_name` are all equally valid identifiers for any kind of binding
(variable, function, type, etc.) — the compiler does not warn or error on casing choice.

*Rationale (pillar 2 vs. pillar 3 tradeoff, made explicit):* this is a deliberate, narrow
exception to pillar 3's "single canonical syntax" — casing style is cosmetic and never
affects program correctness or compiler acceptance, unlike the syntax-level aliasing pillar
3 actually targets (e.g. permitting both `fn` and `func` as the same keyword, which Ofan
does not do). Enforcing casing was considered and rejected specifically because it adds
friction with no correctness payoff, in the same spirit as earlier decisions in this spec
(`as` over `cast<Type>()` in §8, `->` over novel alternatives in §6) that avoided spending
learning-curve budget where it doesn't earn its keep.

*Style guidance, not enforcement:* a **suggested** convention (e.g. `snake_case` for
variables/functions, `PascalCase` for types) belongs in a project style guide or formatter
default, not in the compiler or this spec's lexer-facing rules. It carries no compiler
warning and is not a lexer concern — out of scope for this document.

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

**Decided, fully.** The sub-item previously open here (the `<...>` overload between
region tags and type generics) is resolved below.

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

**Decided sub-item:** `<...>` is **not** an overload. Region tags (`r1`, `r2`, ...) and
type generics (`T`, `E`, ...) are a single underlying concept — **compile-time
parameters** — listed together in one `<...>` list with no positional convention and no
syntactic marker required to distinguish them:

```ofn
struct Cache<r1, T> {
    source: &r1 str,    // r1's role (region tag) is inferred from this usage: it
                          // appears immediately after '&'
    value: T,             // T's role (type) is inferred from this usage: it appears
                          // in a value-type position
}
```

A parameter's specific role — region tag vs. type — is **inferred from how it is used
inside the item's body**, the same mechanism that already infers ordinary lifetimes from
usage in the zero-annotation case earlier in this section. There is no rule requiring
region tags to be listed before or after type parameters, and no sigil or keyword
distinguishes them in the list itself.

*Rationale (pillars 2, 3, 5):* two alternatives were considered and rejected:
- **Positional convention** (region tags always first, by unenforced convention): rejected
  because it asks the *reader*, not just the parser, to know an implicit rule that isn't
  visible in the syntax — the same shape of problem rejected for statement-termination via
  ASI in §3.
- **Syntactic marker** (e.g. a sigil or keyword prefix on region tags inside `<...>`):
  rejected because it reintroduces the symbol-soup this section's core mechanism was
  designed to avoid in the first place (see the `'a`-sigil rejection above) — it would
  partially undo the win of treating region tags as ordinary bare words.

Treating both as one category, with role inferred from use, requires learning nothing new
beyond what §7's core mechanism already teaches (pillar 2), removes the overload entirely
rather than just disambiguating it (pillar 3 — there is one concept, not two sharing a
bracket), and produces a concrete, answerable diagnostic when a parameter's role cannot be
determined: a compile-time parameter listed in `<...>` but never used anywhere in the
item's body produces an "unused compile-time parameter" error, with a suggestion to either
use it or remove it from the list — consistent with how unused-variable diagnostics
already work in comparable languages (pillar 5).

> **See also:** [§13](#13-pointers-and-raw-memory) — if raw pointers are eventually given
> their own region tracking, the inferred-role mechanism resolved above should extend to
> that case too, rather than requiring a separate convention. §13's `unsafe` question
> remains open independently.

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

**Decided.**

```ofn
let x: &i32 = &y;               // safe borrow (decided, see §7)

let p: *i32 = unsafe { raw_ptr() };   // raw/unsafe pointer — requires unsafe { }

let b: Box<i32> = Box::new(5);  // owning heap pointer — ordinary generic type

fn read_register(addr: *u32) -> u32 {
    let value: u32;
    unsafe {
        value = read_volatile(addr);   // only the risky operation is wrapped —
    }                                    // the rest of the function stays unmarked
    value
}
```

`*` is reserved **only** for raw pointers — never reused for multiplication in a
position-dependent way. This avoids the known C readability wart where `*` means
"pointer type" in a declaration and "dereference" in an expression.

**`unsafe { }` is required around any raw-pointer dereference or raw-pointer-producing
operation.** `unsafe` is a confirmed reserved keyword.

*Scope:* **block-scoped, not function-scoped.** `unsafe` marks only the specific region
where the safety guarantee is suspended — not the entire enclosing function. A function
may freely mix safe code and one or more small `unsafe { }` blocks; only the blocks
themselves carry the marker.

*Rationale (pillar 1):* a type-level signal alone (`*i32` in a variable's type annotation)
is real but easy to miss when skimming a function body, since the type only appears once,
at the declaration site — not at every point the value is actually used dangerously. A
block-level marker is visible without reading any types at all, and is mechanically
searchable (e.g. trivially `grep`-able across a codebase) in a way a type annotation alone
is not — directly serving pillar 5's "errors and risk should be locatable, not just
theoretically present" spirit.

Block-scoping rather than function-scoping was chosen deliberately: marking an entire
50-line function `unsafe` because one line dereferences a raw pointer would force a reader
to treat all 50 lines as equally suspect, which *weakens* the signal pillar 1 is trying to
provide. A block exactly as large as the actual risk keeps the marker meaningful.

> **See also:** [§7](#7-lifetime--region-inference-and-escape-hatch) — if raw pointers
> ever need region tracking, §7's inferred-role mechanism for compile-time parameters
> should extend to cover that case rather than introducing a new convention.

---

## §14 Numeric literals

**Decided.**

```ofn
let big: i32 = 1_000_000;     // digit separators, ignored by the lexer
let hex: u32 = 0x4002_0014;   // hex — load-bearing for microcontroller/register code
let bin: u8 = 0b1010_1100;    // binary — register flags, bit manipulation
let oct: u32 = 0o755;         // octal — kept for completeness, no extra lexer cost

let inferred = 5;              // defaults to i32 if nothing else constrains it
let pi = 3.14;                 // defaults to f64 if nothing else constrains it
let typed: u8 = 5;              // type comes from the annotation, not a literal suffix
```

*Digit separators (`_`):* ignored by the lexer wherever they appear inside a numeric
literal. Unambiguous — an identifier can never start with a digit, so `1_000` cannot be
confused with `1` followed by an identifier. Adopted for readability at zero cost
(pillar 2), with no viable alternative reading to rule out.

*Alternate bases (`0x`, `0b`, `0o`):* `0x` (hex) and `0b` (binary) are not optional
polish — they are directly load-bearing for the microcontroller/register-manipulation
cluster of Ofan's launch niche (see `PHILOSOPHY.md` §5.2), where raw addresses and bit
flags are written in these forms constantly. `0o` (octal) is kept despite being rarely
needed in modern code, since the prefix-dispatch lexer logic already required for `0x`/
`0b` extends to `0o` at no additional cost — there is no readability or safety argument
for omitting it once the mechanism exists.

*No literal type suffixes (no `5i32`, `5u8`, `3.14f32`):* rejected as a second, redundant
way to specify a type that would compete with the annotation system already locked in §5
("type annotation optional when inferable"). A literal's type comes from its context
(the variable's declared type, a function parameter's type, etc.).

*Default type when unconstrained:* `i32` for integers, `f64` for floats — the same
defaults Rust uses in the equivalent situation, adopted specifically because they carry
no learning-curve cost for anyone arriving with prior systems-language experience
(pillar 2), and deviating would only create friction without a corresponding benefit.

---

## §15 String and character literals

**Decided (core escape set and character literals). Two extensions explicitly deferred —
see below.**

```ofn
let greeting: str = "hello\n";          // standard double-quoted string
let path_sep: str = "a\\b";             // escaped backslash
let nul: str = "name\0";                // null byte — relevant at FFI/C-interop boundaries
let letter: char = 'a';                 // single-quoted character literal
let quote_char: char = '\'';            // escaped single quote inside a char literal
```

*Escape set:* `\n` `\t` `\r` `\\` `\"` `\0` for string literals; the same set applies to
character literals with `\'` in place of `\"` (a character literal needs to escape its own
delimiter, not the string delimiter). This is the minimum set every systems language
needs — adopting it as-is rather than inventing alternatives follows the same logic
already used for `as` (§8) and `->` (§6): deviating from a near-universal baseline buys
nothing and costs unnecessary learning-curve friction (pillar 2).

*Unrecognized escape sequence (e.g. `\q`):* a **hard lexer error**, never silently passed
through or silently dropped. Per pillar 1, an unrecognized escape must not be treated as
ordinary text — and per pillar 5, the resulting error must name the invalid sequence and
list the valid alternatives (e.g. "unknown escape sequence `\q` — valid escapes: `\"` `\\`
`\n` `\t` `\r` `\0`"). This matches behavior already implemented on the
`feat/lexer-implementation` branch's `InvalidEscape` error variant — this section
confirms that existing behavior as the canonical decision rather than introducing
anything new.

*Character literals (`'a'`):* a single-quoted literal containing exactly one character,
distinct from a one-character string. Standard across systems languages (Rust, C, Java)
and kept for the same reason — no deviation, no learning-curve cost, and a genuinely
distinct type (`char` vs `str`) is useful for byte/codepoint-level work relevant to
Ofan's launch niche.

**Deferred — explicitly not decided in this pass:**
- **Unicode escapes** (e.g. `\u{1F600}`-style insertion of a codepoint by number): low
  priority for Phase 1's microcontroller/no-std/speedcoding focus, where arbitrary
  Unicode-by-codepoint is rarely needed. Deferred rather than rushed, given pillar 5's
  requirement that any escape syntax added must have well-defined error behavior for
  malformed input, which deserves its own dedicated pass rather than a decision made
  alongside several other items at once.
- **Raw strings** (no escape processing at all, e.g. for embedding paths, regex, or raw
  byte sequences): genuinely useful for a systems language, but not urgent for Phase 1.
  Deferred for the same reason as Unicode escapes — worth a focused decision, not a
  rushed one.

---

## §16 Not yet decided — deferred

The following constructs have appeared informally in design examples, or were surfaced
during review, but have **never been formally decided**. Do not assume any particular
syntax is settled for these.

**Lexer-relevant (deferred deliberately, not overlooked):**
- **Unicode escapes** (`\u{...}`-style) — see §15
- **Raw strings** (no escape processing) — see §15

**Parser/typechecker-relevant (out of scope for the lexer's first pass; do not block it):**
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
- **Array/slice literal syntax** — `[f64]` and `&[f64]` used informally in earlier
  examples; literal construction syntax (`[1, 2, 3]`?) and fixed-size vs. dynamic-size
  distinction never formalized
- **Explicit generic instantiation at a call site** — when a generic function's type
  parameter can't be inferred from arguments, no syntax has been decided for specifying
  it explicitly at the call site (Rust's `::<T>` "turbofish" solves this; Ofan has no
  equivalent yet)
- **Void/unit type** — functions with no return type currently just omit `->` (§6), but
  whether a first-class "no value" type exists (for use as a generic parameter, for
  example) has not been decided

**Process note, not a syntax item:** there is currently no master list tracking which
identifiers are *reserved* as future keywords before their syntax is finalized (e.g. is
`loop` already implicitly spoken-for on `feat/lexer-implementation` even though loop
syntax itself is undecided here?). Worth resolving as a coordination step between this
spec and the in-flight lexer branch, not as a syntax decision in its own right.

These do not block lexer work on the tokens that *are* decided in §1–§15, but the token
set will need a follow-up pass once the parser/typechecker-relevant items above are
resolved.

---

*Source: content migrated from `docs/prds/2026-06-26-lexer.md` during the 2026-06-26
documentation reorganization. Extended in a follow-up session the same day to resolve
§1, §2, and the §7/§13 sub-items, and to add §14 (numeric literals) and §15 (string/char
literals).*