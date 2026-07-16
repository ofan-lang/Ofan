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
| [16](#16-loop-syntax) | Loop syntax | Decided |
| [17](#17-copymove-semantics) | Copy/Move semantics | Decided |
| [18](#18-method-receiver--self-and-self) | Method receiver — `self` and `Self` | Decided |
| [19](#19-option-and-checked-types-and-variant-names) | `Option` and `Checked` — types and variant names | Decided |
| [20](#20-enum-declaration-syntax) | Enum declaration syntax | Decided |
| [21](#21-match--pattern-matching) | Match / pattern matching | Decided |
| [22](#22-impl-block-syntax) | `impl` block syntax | Decided |
| [23](#23-struct-field-access) | Struct field access | Decided |
| [24](#24-not-yet-decided--deferred) | Not yet decided — deferred | — |

---

## Status summary

At a glance: **23 of 24 numbered sections decided** (§15 has two narrow, deliberately
deferred extensions — Unicode escapes and raw strings — that do not block the core
lexer work). Every token-level construct needed for a first lexer implementation is now
covered. §18 covers `Self` (capital) — the impl-block type alias. §22 now formally
specifies `impl` block structure, multiplicity, and conflict detection. §23 specifies
struct field access. The remaining open ground is §24 — constructs that have never been
formally designed at all (traits, modules, attributes, array literals, generic call
syntax, void/unit type) — which were always out of scope for the lexer's first pass
and do not block it.

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

**Decided: no compiler-enforced casing rule. Structural identifier grammar only.
Character set: ASCII-only.**

An identifier is a contiguous run of **ASCII letters (`a`–`z`, `A`–`Z`), ASCII digits
(`0`–`9`), and underscores (`_`)**; it must not start with a digit; it cannot contain
whitespace or any reserved operator/punctuation character. This is the same
near-universal structural constraint every lexer requires (the reason `My variable`,
`var&&var`, `var#var`, and `Hello/world` are invalid is that each would either split into
multiple tokens or collide with an operator — not a casing issue at all).

*Spec-gap closure (2026-06-28):* the previous text said "letters, digits, and underscores"
without specifying whether "letters" meant ASCII-only or Unicode-permitting. The
implementation (`src/lexer/mod.rs`) had already chosen ASCII-only — via literal char
ranges `'a'..='z' | 'A'..='Z' | '_'` at the start position and
`c.is_ascii_alphanumeric() || c == '_'` in the continuation loop — but that choice was
implicit, not derived from a spec decision. This section makes it explicit. No code
changes required; the implementation matches this decision exactly.

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

**Deferred — explicitly not decided in this pass:**
- **Unicode-permitting identifiers** (accepting Unicode letters per `XID_Start`/
  `XID_Continue` categories, as Rust, Python 3, and Java do): considered and deferred for
  Phase 1. The launch niche (microcontroller/no-std/speedcoding) is overwhelmingly ASCII
  source, so the complexity cost has no near-term payoff — and that cost is real:
  Unicode-permitting identifiers require resolving two non-trivial sub-questions (which
  Unicode normalization form is canonical for identifier comparison? how are visually
  confusable codepoints, e.g. Cyrillic `а` vs Latin `a`, handled to satisfy pillar 1's
  "never silent erroneous behavior" guarantee?) that together deserve a dedicated design
  pass rather than being decided as a rider on this spec-gap closure. If
  Unicode-permitting identifiers are revisited later, this section is the entry point.

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
let big: i32 = 1_000_000;     // digit separators, stripped by lexer
let hex: u32 = 0x4002_0014;   // hex — load-bearing for microcontroller/register code
let bin: u8 = 0b1010_1100;    // binary — register flags, bit manipulation
let oct: u32 = 0o755;         // octal — kept for completeness, no extra lexer cost

let inferred = 5;              // defaults to i32 if nothing else constrains it
let pi = 3.14;                 // defaults to f64 if nothing else constrains it
let typed: u8 = 5;              // type comes from the annotation, not a literal suffix
```

*Digit separators (`_`):* a single `_` may appear between any two digits within a
numeric literal, in any base. Unambiguous — an identifier can never start with a digit,
so `1_000` cannot be confused with `1` followed by an identifier. Adopted for
readability at zero cost (pillar 2).

Placement is enforced: `_` is not valid at the start of the digit sequence (e.g.
`0x_FF`), at the end (`1000_`), or doubled (`1__000`). Violations are a hard lexer
error per pillar 1, with the message:
"misplaced `_` in numeric literal at byte {byte} — digit separators are valid only
between two digits (e.g. `1_000`), not at the start, end, or doubled"

Valid: `1_000`, `0xFF_00`, `0b1010_0101`, `0o17_77`, `3.141_592`
Invalid: `1000_`, `1__000`, `0x_FF`, `0xFF_`, `0x1__2`, `1.5_`, `1.5__3`

Note: a leading `_` on the whole literal (e.g. `_1000`) is not a malformed number —
`_` triggers the identifier arm and lexes as `Ident("_1000")`. No special case needed.

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

*Numeric literal immediately followed by an identifier-start character:* a hard lexer
error. After a numeric literal scan completes successfully, if the next character is an
ASCII letter or `_` (an identifier-start character in Ofan — see §2), the lexer emits:

```
numeric literal `{literal}` at byte {start} is immediately followed by `{ch}`
— Ofan has no literal suffixes (§14); if these are separate tokens, add whitespace
between them
```

*Why at the lexer and not the parser:* since §14 already disallows literal type suffixes,
a number immediately abutting an identifier character can never be valid Ofan. Catching
it at the lexer produces a message that names both the literal and the offending
character; letting it fall through to the parser would produce a generic "unexpected
identifier" message with no memory of the preceding numeric literal. `0x1fg` is the
motivating case: `f` is a valid hex digit and is silently consumed into the literal,
leaving `g` as a stray identifier with no explanation at the parser level — a silent
value corruption the programmer may not notice (`0x1fg` was likely intended as a unit,
a typo, or a note that is not a hex digit at all).

*Precedence relative to `MisplacedDigitSeparator`:* first-problem-encountered, scanning
left to right — not a priority ranking between the two checks. The new check runs only
after the number scan completes successfully. Inputs like `1_abc` already error on
`MisplacedDigitSeparator` (the `_` is consumed by the scan loop, which checks the
next character as a digit lookahead and fails) before the success exit is reached.
The two checks are sequenced by control flow, not ranked by design.

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

## §16 Loop syntax

**Decided: three loop forms (`while`, `loop`, `for`), standard `break`/`continue`,
`break value` permitted only inside `loop`, and `for`'s iteration binding directly
inherits the borrow and Copy/Move model locked in §7 and §17 — no new mechanism
introduced.**

```ofn
while count < 10 {
    count = count + 1;
}

loop {
    if should_stop() { break; }
}

// loop as an expression — break can carry a value, ONLY inside loop
let result = loop {
    count = count + 1;
    if count == 10 { break count * 2; }
};
// result == 20

for item in items {
    // consumes items's elements per §17's Copy/Move rule applied to each element
}

for item in &items {
    // borrows each element — items remains usable by the caller after the loop
}

for item in &mut items {
    // mutably borrows each element — may modify elements in place;
    // items remains the same binding after the loop, only its contents may have changed
}
```

**The three forms:**

`while` — repeats while a boolean condition holds. No parentheses around the condition,
consistent with `if`'s established form throughout the language. Braces are mandatory
per §4.

`loop` — unconditional repetition; exits only via `break`. This form exists as its own
keyword rather than `while true { ... }` because the intent — "this loop deliberately
has no condition; it runs until explicitly broken" — is carried by the keyword itself,
not derivable only after reading and evaluating a condition. A reader scanning a function
body reaches `loop` and immediately knows the loop's exit structure must be a `break`
somewhere in the body; `while true` requires reading the condition before that same
conclusion is reachable. Per pillar 1 (explicit, never silent erroneous behavior), making
structural intent visible at the first token rather than reconstructible only from context
is the same principle that distinguishes `&mut self` from an unmarked mutation in §18 —
the safety-relevant information should be at the point a reader first encounters the
construct, not inferred after the fact.

`for` — iterates over a collection, binding each element (or each borrow of an element)
to a name per iteration. `in` is the separator between the binding name and the
collection expression.

**`break` and `continue`:** standard semantics, valid across all three forms. `break`
exits the innermost enclosing loop immediately. `continue` skips to the next iteration
of the innermost enclosing loop. Both `break` and `continue` are already reserved
keywords in the lexer.

**`break value` — `loop`-only:**

A `loop` is itself an expression. `break expr` causes the entire `loop` expression to
evaluate to the value of `expr`. This is **not** valid for `while` or `for`.

The asymmetry is not arbitrary. A `while` or `for` loop has two exit paths: an explicit
`break` and a natural exit (condition becomes false, or the iterator is exhausted). A
natural exit produces no value by definition — there is nothing to return when the loop
simply ran out of items or its condition failed. Allowing `break value` on `while` or
`for` would require deciding what the expression evaluates to on a natural exit (unit?
`Option<T>`? an error?), a distinct design question not resolved here, which different
languages handle differently. `loop` has exactly one exit path — `break` — so `break
value` is unambiguous: every exit is an explicit break, and that break always carries
the value. Restricting `break value` to `loop` is not a limitation but a precision: it
is the only form where the expression-with-value semantics are clean without introducing
an implicit second return path that would require a separate decision.

**`for` iteration forms and the §7/§17/§18 model — second validation:**

`for item in items` — each element is transferred or duplicated from the collection per
§17's Copy/Move rule, applied element by element. For a collection whose element type is
`Move`, each element is moved out of the collection; the collection cannot be used after
the loop (its elements have been consumed, ownership transferred to the loop body on each
iteration). For a collection whose element type is `Copy`, each element is duplicated;
the collection itself remains usable after the loop.

`for item in &items` — each element is borrowed immutably per §7's borrow rules.
`item` inside the loop body is a reference to an element (`&T`). `items` remains fully
usable by the caller after the loop completes, because the loop held only a borrow, not
ownership.

`for item in &mut items` — each element is borrowed mutably. The loop body may modify
elements in place. `items` remains the same binding after the loop; only its contents
may have changed. The collection is not consumed — this is a mutable borrow, not a move,
applying the same mechanism as `&mut self` in §18 at the iteration position rather than
the method-receiver position.

**No new mechanism introduced.** The three `for` forms apply `&`/`&mut`/bare-value at
the iteration position — the exact same pattern already locked in §7 (borrow syntax),
§17 (Copy/Move semantics), and §18 (method receiver forms). Nothing new is required to
specify `for`'s ownership behavior; the existing model covers it completely.

This is a **second validation** that the ownership model generalizes cleanly across
syntactically different positions. §18 was the first: method receivers applied
`&`/`&mut`/bare-value at the method boundary with no special-casing required. `for`
applies the same pattern at the iteration boundary — a syntactically different position,
semantically identical treatment. The model has now generalized across four distinct
syntactic positions (struct field types in §17, function parameters in §7, method
receivers in §18, loop iteration here) with zero exceptions or special cases at any of
them. This is the expected signature of a well-designed ownership model, and is worth
naming explicitly rather than leaving it as a pattern a reader must discover
independently.

> **See also:** [§7](#7-lifetime--region-inference-and-escape-hatch) — borrow rules
> govern `for item in &items` and `for item in &mut items`. [§17](#17-copymove-semantics)
> — Copy/Move rule determines element ownership in `for item in items`.
> [§18](#18-method-receiver--self-and-self) — the same `&`/`&mut`/bare-value pattern applied at
> the method-receiver position; §18 is the first prior validation of the model's
> generalization to a new syntactic position.

---

## §17 Copy/Move semantics

**Decided: Move-by-default, with compiler-inferred Copy for structurally provably-safe
structs, and explicit `copy`/`move` keywords that override inference in either direction.**

```ofn
struct Point { x: f64, y: f64 }
// automatically Copy — every field is a primitive, nothing here could hide a resource

struct Entity { x: f32, y: f32, velocity_x: f32, velocity_y: f32 }
// also automatically Copy — same reasoning

move struct FileHandle { fd: i32 }
// fd is structurally just an i32 (would auto-infer Copy), but the programmer knows it
// represents a resource handle and overrides inference explicitly

struct Cache<r1, T> { source: &r1 str, value: T }
// NOT automatically Copy — contains a borrow and a generic, neither provably safe to
// duplicate without more information. Defaults to Move.

copy struct SafeCache<r1, T> { source: &r1 str, value: T }
// programmer can override to Copy if they know it's safe in their case
```

**The rule** — three parts, applied in priority order:

1. If the struct declaration is prefixed with `copy` or `move`, that declaration always
   wins — no inference is performed. The programmer's intent overrides the structural
   analysis.
2. Otherwise, if every field's type is itself provably `Copy` — primitives (`i32`, `f64`,
   `bool`, `char`, and the other fixed-width scalar types) or another struct already proven
   `Copy` by this same rule, recursively — the struct is automatically treated as `Copy`.
3. Otherwise, the struct is `Move`.

A `Copy` binding duplicates freely: both the original and any copy remain valid after an
assignment or function call. A `Move` binding transfers ownership: the original binding
becomes invalid after it is assigned or passed to a function, and any subsequent use is a
compile error.

**Heuristic warning for residual risk:** when a struct is auto-inferred as `Copy` and
contains a field named `fd`, `handle`, or any name beginning with `ptr`, the compiler
emits a warning (not an error — this is a heuristic, not a semantic guarantee) suggesting
an explicit `move` override:

```
warning: struct 'FileHandle' was inferred as Copy because all fields are primitive
types, but field 'fd' looks like it may represent a resource handle — if duplicating
this value should transfer ownership instead, mark it explicitly:
'move struct FileHandle { ... }'
```

Fields named `id` are deliberately excluded from this heuristic. Plain-data structs with
an `id` field (entity IDs in game-dev code, index types in microcontroller code) are common
enough in Ofan's launch niche that including `id` would produce a high false-positive rate
and erode trust in the warning. The heuristic targets only names structurally associated
with OS-level resource handles — `fd` (file descriptor), `handle` (Windows resource handle
pattern), and `ptr`-prefixed names — not integer IDs.

*Rationale — six alternatives were considered and rejected:*

**Always-copy (C/C++ default):** rejected because it silently duplicates ownership for any
resource-owning struct. A `FileHandle` copied and used twice means two copies both try to
close the same descriptor at cleanup — a classic double-close bug, undefined behavior in C,
silent and undetectable. This directly undermines pillar 1's guarantee that erroneous
behavior is never silent.

**Always-move (strict, no Copy at all):** rejected because it forces ceremony — a manual
clone-equivalent call — even for trivially-safe data like `Point { x: 0.0, y: 0.0 }`,
which is the dominant case in Ofan's Phase 1 launch niche (microcontroller register structs,
game-dev entity data). Applying the same restriction to plain data provides no safety
benefit and adds friction on every common-case use. Maximally safe, but it optimizes for
the rare, dangerous case at the expense of the common, safe case.

**Move-by-default, `copy` as the only override (no inference):** safe, and was the
fallback position if a smarter option couldn't be found. Rejected because requiring a
`copy` prefix on every plain-data struct makes the common case pay ceremony for no safety
gain. Structural inference removes that ceremony for the common case without opening any
new risk, since its safety criterion (all fields provably `Copy`) is mechanically
verifiable.

**Copy-by-default, `uncopy`/`move` as the override:** rejected because forgetting the
override on a resource-owning struct silently grants Copy, which is exactly the silent
unsafe duplication pillar 1 forbids. This alternative inverts which case pays the ceremony
cost — the rare case (resource-owning structs) instead of the common case (plain data),
which is ergonomically appealing — but the failure modes are not symmetric: Move-by-
default's analogous failure (forgetting `copy` on a plain-data struct) costs only an
annoying but safe compile error, whereas Copy-by-default's failure is a silent correctness
bug. Pillar 1 forbids the first class of failure and can tolerate the second.

**Pure structural inference with no override at all:** rejected because it removes the
programmer's authority over semantic cases the type system cannot see. `FileHandle { fd: i32 }`
is the direct example: `fd` is structurally an `i32`, provably `Copy`, but semantically a
resource the compiler cannot recognize as such. Inference without override would silently
grant Copy to `FileHandle` with no recourse, violating the requirement that the programmer
always has final authority over a struct's Copy/Move status.

**"Always explicit, no default at all" (`copy struct`/`move struct` required, bare `struct`
not valid):** rejected because it imposes mathematically identical typing cost to
Move-by-default (every struct pays the annotation), while removing the ergonomic comfort of
a default for the common case. Strictly worse learnability for no safety gain relative to
the adopted model.

The adopted model — inferred-Copy when provably safe, Move by default, explicit override in
either direction, heuristic warning for the narrow residual risk — is the only option that
gives the dominant real-world case zero ceremony while confining the residual pillar 1 risk
to a narrow, named, partially-mitigated case rather than accepting it silently across all
structs.

> **See also:** [§18](#18-method-receiver--self-and-self) — Copy/Move structural inference
> extends to receiver access mode: `self` as receiver infers its borrow level from body
> usage; `move self` is the consuming override, parallel to `move struct` here.

---

## §18 Method receiver — `self` and `Self`

**Decided: `self` receiver access mode is inferred from method body usage — the same
inference mechanism as §17 Copy/Move — with `move self` as the explicit consuming
override. `Self` (capital) in type position is a name-resolution alias for the enclosing
`impl` type, not part of the borrow/ownership mechanism.**

```ofn
impl Entity {
    fn distance_to(self, other: &Entity) -> f32 {
        # read-only — inference sees no field mutation, no move of self
        # → inferred as immutable borrow: caller's binding unaffected after this call
        ...
    }

    fn update(self, dt: f32) {
        self.x = self.x + self.velocity_x * dt;
        # self.x assigned — inference sees field mutation
        # → inferred as mutable borrow: caller's binding same binding, contents changed
    }

    fn into_id(move self) -> u32 {
        self.id
        # explicit move: programmer forces full ownership transfer into the method
        # → caller's binding invalid after this call (if Entity is Move per §17)
    }

    fn clone(self) -> Self {
        # Self in return-type position resolves to Entity — name alias, not borrow mechanism
        Entity { x: self.x, y: self.y, ... }
    }
}
```

**Receiver access mode inference:**

`self` as a method parameter has its access mode — immutable borrow, mutable borrow, or
consuming — inferred by the compiler from how the body uses it. The same structural
inference mechanism from §17 (Copy/Move) applies here: the compiler determines the minimal
sufficient access level that makes the body well-typed.

- If the body only reads `self`'s fields and passes `self` to other shared-reference
  parameters, inference produces an immutable borrow. The caller's binding is fully usable
  after the call, unchanged.
- If the body assigns any field of `self` (`self.field = ...`) or passes `self` to a
  mutable-borrow parameter, inference produces a mutable borrow. The caller's binding is
  the same binding after the call — only its contents may have changed.

The programmer does not write `&` or `&mut` at the receiver position. These forms do not
exist in Ofan source code. The inferred access level is compiler-visible information, not
source-level syntax. Per pillar 3, there is one way to write a method receiver in shared
source: bare `self`.

**Consuming receiver — `move self`:**

When the method requires full ownership of the receiver, the programmer writes `move self`
explicitly — the same override keyword that `move struct` uses in §17. This forces a
consuming receiver regardless of what body analysis would otherwise infer:

- For a Move struct (§17): the caller's binding becomes invalid after the call.
- For a Copy struct (§17): the caller's binding is untouched — a duplicate was consumed.

`move self` is the one case that requires explicit annotation. Everything else is inferred.

**Ambiguity — hard compile error, never silent fallback (pillar 1):**

If inference cannot determine a single minimal access level — for example, because
conflicting requirements arise across branches, or because dispatch is unresolved in a
generic context — this is a **compile error**, never a silent fallback to a "safe" default.

Example error shape (pillar 5 — context + suggestion required):

```
error: cannot infer access mode for `self` in `Entity::process`
  → line 14: `self.update(dt)` requires mutable borrow of `self`
  → line 17: `consume(self)` would move out of `self`
note: these requirements conflict — the method cannot simultaneously borrow and consume
suggestion: if consuming ownership is intended, write `move self` and restructure the body
            so the borrow at line 14 precedes the move at line 17
```

The error must name the specific conflicting usage sites in the body, not just the `self`
parameter declaration. Generic "type error at `self`" messages violate pillar 5.

**`Self` type:**

`Self` (capital) inside an `impl` block is a name-resolution alias for the type the block
implements. It has no borrow, ownership, or inference semantics — it is resolved at
name-resolution time exactly like any named type, to the enclosing `impl`'s type.

```ofn
impl Entity {
    fn clone(self) -> Self { ... }     # Self resolves to Entity
    fn default() -> Self { ... }       # Self resolves to Entity
}
```

`Self` is not valid outside an `impl` block. It is not a keyword token (`self` lowercase
is a keyword; `Self` uppercase resolves through the type namespace, distinguished by
convention). `Self` and `self` (lowercase) are unrelated in the type system: `self` is a
parameter name that triggers receiver inference; `Self` is a type alias. They must not be
conflated.

*Rationale (pillar alignment):*

**Pillar 3 (single canonical syntax):** inference removes the `&self`/`&mut self`/`self`
three-way syntactic choice from the programmer. There is one form in shared source — bare
`self` — and one override form — `move self`. No second valid way to express a read-only
or mutating receiver exists in persisted source.

**Pillar 1 (explicit erroneous behavior):** inference-with-hard-error-on-ambiguity is
safer than explicit annotation, not less safe. Under an explicit design, a programmer who
wrote the wrong form got a compile error pointing at the mismatch — the error message had
to explain what the access level was. Under inference, the compiler makes the same
determination without requiring a redundant annotation. Ambiguous cases are promoted to
hard errors with conflict-site pointing. The pillar 1 guarantee is strictly preserved.

**§17 validation (third position):** `move self` is the third confirmation that §17's
`copy`/`move` keyword pattern generalizes cleanly — `copy struct`, `move struct` (§17),
`for &x in collection` (§16 iteration), `move self` (§18). One keyword form, one override
pattern, three positions validated without special-casing.

> **See also:** [§17](#17-copymove-semantics) — the Copy/Move structural inference
> mechanism is the same one applied here to receiver access mode; `move self` mirrors
> `move struct`. [§22](#22-impl-block-syntax) — `impl` block structure, multiplicity,
> and conflict detection. [§24](#24-not-yet-decided--deferred) — trait/interface syntax
> (how `impl` blocks interact with named traits) remains unresolved and does not block this.

---

## §19 Option and Checked types and variant names

**Decided: two standard result-handling types — `Option<T>` for values that may be
absent, and `Checked<T, E>` for operations that may fail — with variant names
`Some(T)`/`None` and `Ok(T)`/`Err(E)` respectively. Neither type's variants are
reserved keywords; they are standard-library constructors available via the prelude.**

```ofn
// Option<T> — a value that may or may not be present
let nickname: Option<str> = user.nickname;
let display = nickname ?: user.full_name;   // ?:  fallback operator, §12

// Checked<T, E> — an operation that may succeed or fail
fn read_config(path: &str) -> Checked<Config, &str> {
    let raw = read_file(path)?;             // ?   propagate operator, §12
    let cfg = parse(raw)?;
    Ok(cfg)
}

fn caller() -> Checked<(), &str> {
    let cfg = read_config("app.ofn")?;
    // match is the only way to branch on Checked — ?:  is deliberately
    // invalid on Checked<T, E> (see §12 and rationale below)
    Ok(())
}
```

**`Option<T>` — `Some(T)` and `None`:**

`Option<T>` represents a value that is either present (`Some(T)`) or absent (`None`).
The name and variant names match Rust, Swift, OCaml, and Scala — the same adoption
logic used for `as` (§8) and `->` (§6): deviating from a near-universal baseline adds
learning-curve friction (pillar 2) with no benefit.

`Some(value)` — wraps a present value. `None` — bare; no payload.

The `?:` fallback operator (§12) is specifically for `Option<T>`: `a ?: b` evaluates
to `b` when `a` is `None`, to the unwrapped value when `a` is `Some(x)`. The `?`
propagate operator (§12) also applies: `expr?` on an `Option<T>` immediately returns
`None` from the enclosing function when `expr` is `None`, otherwise unwraps to `T`.

*Alternatives rejected:*
- **`Maybe<T>` / `Just(T)` / `Nothing`** (Haskell): less intuitive for systems
  programmers and adds no expressiveness. `Option`/`Some`/`None` already carry the
  meaning clearly.
- **`Present(T)` / `Absent`**: more verbose, no precision gained over `Some`/`None`.

**`Checked<T, E>` — `Ok(T)` and `Err(E)`:**

`Checked<T, E>` represents an operation that either succeeded (`Ok(T)`) or failed with
an error (`Err(E)`). `E` may be any type — a string, a dedicated error enum, or a
struct.

The type is named `Checked`, not `Result`, for a precise pillar 1 reason: `Result` is
a neutral name for a two-variant type; `Checked` signals that the value represents an
operation that was checked for correctness and that a programmer receiving this type
must explicitly inspect it before using the success value. The name carries the intent
— "this was checked; now you must check it too" — in the same way that `loop` (§16)
makes the intent "this has no condition, it exits only via break" visible at the
keyword rather than derivable only from reading the body.

The variant names are `Ok`/`Err`, not renamed: pillar 2 applies here. The pillar 1
signal is in the type name; deviating from the near-universal `Ok`/`Err` spelling
would add friction for any programmer arriving from Rust, Go-adjacent idioms, or
general functional programming without any compensating clarity gain. The variants are
the obvious spelling of "success" and "error" — changing them would be novelty for
its own sake.

**`Checked<T, E>` and `?:` (pillar 1 enforcement):** the fallback operator `?:` is
explicitly **not** valid on `Checked<T, E>` (see §12). Allowing a fallback to silently
discard an `Err` would violate pillar 1 — silent error discard is the exact failure
mode `Checked` exists to prevent. `match` is the only way to branch on a `Checked`
value's failure case and supply an alternative, by deliberate design: match forces the
programmer to name the error case explicitly before providing a replacement value.

*Alternatives rejected:*
- **`Result<T, E>` / `Ok` / `Err`**: the variant names are adopted unchanged; only
  the type name is changed. `Result` was rejected because the name is semantically
  neutral — it carries no obligation signal and does not distinguish "the result of an
  operation" from "the result of an operation that must be checked for errors."
- **`Either<L, R>` / `Left` / `Right`**: functional-programming convention; left/right
  carries no success/failure semantics and requires additional convention to interpret.
  Adds learning cost with no precision gain over `Ok`/`Err`.

**Constructors, not keywords:** `Ok`, `Err`, `Some`, `None` are standard-library
value constructors available in the prelude — they are not reserved keywords and do not
require changes to the lexer's keyword table. They lex as ordinary identifiers
(`Token::Ident`). The type-checker, not the lexer, gives them special meaning.

> **See also:** [§12](#12-operators) — `?` (propagate) and `?:` (fallback) operators
> are defined in terms of `Checked<T, E>` and `Option<T>`; §12's rationale for why
> `?:` is excluded from `Checked` is the companion to this section's `Checked` naming
> rationale.

---

## §20 Enum declaration syntax

**Decided: `enum` keyword, braces, comma-separated variants with a trailing comma
permitted. Two variant forms: unit (no payload) and tuple (positional payload).
Generic enums use the same `<T>` compile-time parameter syntax as generic structs
(§7). Copy/Move semantics follow the same rule as structs (§17) — a fourth
validation of the same model. Struct variants (named fields inside a variant) are
explicitly deferred, not rejected.**

```ofn
enum Direction { North, South, East, West }

enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Point,                    // trailing comma permitted
}

enum Option<T> {
    Some(T),
    None,
}

enum Checked<T, E> {
    Ok(T),
    Err(E),
}

move enum Handle {
    Fd(i32),
    Null,
}
```

**Declaration form:**

`enum Name { ... }` — braces are mandatory per §4. Variants are comma-separated;
a trailing comma after the last variant is permitted (reduces diff noise when
variants are added or reordered). The `enum` keyword is already reserved (`Token::Enum`)
and is consistent with Rust, Swift, Kotlin, TypeScript, and C — near-universal
baseline, no deviation needed (pillar 2).

**Two variant forms:**

*Unit variant* — no payload; bare name only. `North`, `None`, `Null`. Constructing:
just the name. A unit variant is a complete value of the enum type on its own.

*Tuple variant* — one or more comma-separated positional payload types in parentheses.
`Circle(f64)`, `Some(T)`, `Ok(T)`. Constructing: the variant name applied as a
function — `Circle(3.14)`, `Some(x)`, `Ok(result)`. Positional, not named.

**Struct variants deferred:** a struct variant embeds named fields directly in the
variant (`Rectangle { width: f64, height: f64 }` instead of `Rectangle(f64, f64)`).
This is a convenience — it is fully expressible today as a tuple variant wrapping a
named struct (`Rectangle(RectData)` where `RectData` is a separate `struct`). Adding
struct variants introduces a third declaration form without new expressiveness.
Deferred, not rejected: if real Ofan code shows this convenience gap is consistently
painful, the decision can be reopened.

**Generic enums:**

Generic enums use `<T>` compile-time parameters with the same role-inference
mechanism as §7. `T` appearing in a value-type position inside a variant body is
inferred as a type parameter — no syntactic marker required, the same rule already
used for generic structs.

`Option<T>` and `Checked<T, E>` (§19) are standard-library enums defined using
exactly this syntax — they are not special compiler types. Any user-defined enum
can be generic in the same way.

**Copy/Move for enums — fourth validation of the §17 model:**

The §17 rule applies to enums without modification:

1. If the enum is prefixed with `copy` or `move`, that always wins.
2. Otherwise: `Copy` if and only if every payload type across every variant is
   provably `Copy`. Unit variants have no payload and are trivially `Copy`.
   A single `Move` payload in any variant makes the whole enum `Move`.
3. Otherwise: `Move`.

```ofn
enum Direction { North, South, East, West }
// automatically Copy — all variants are unit; no payload at all

enum Shape { Circle(f64), Point }
// automatically Copy — f64 is Copy; Point is unit

move enum Handle { Fd(i32), Null }
// i32 is Copy so Handle would auto-infer Copy, but Fd represents a resource
// handle — override explicitly, same reasoning as §17's move struct FileHandle
```

The §17 heuristic warning (`fd`, `handle`, `ptr`-prefixed names) cannot fire on
tuple variants by name — tuple payloads are positional and have no field names. The
programmer must use `move enum` explicitly when a tuple variant's payload represents
a resource. This is not a gap in the rule; it is the correct consequence of tuple
variants being positional. The override mechanism covers it.

This is a **fourth validation** that the §17 model generalizes. §18 was the first
(method receivers), §16 was the second (for iteration), §19/§20 is a third/fourth
(Option/Checked and now all enums). The Copy/Move rule has now applied correctly at
every type-declaration position in the language — struct fields, enum variants, method
receivers, loop iteration — with zero special cases.

**Methods on enums:**

Enums can have `impl` blocks and methods using exactly the same mechanism decided
in §18 (receiver forms) and §22 (block structure, multiplicity, conflict detection).
`self` receivers work identically for enums and structs — no new syntax or
special-casing at the enum position.

> **See also:** [§7](#7-lifetime--region-inference-and-escape-hatch) — compile-time
> parameters `<T>` for generic enums. [§17](#17-copymove-semantics) — Copy/Move rule
> extended to enums here. [§18](#18-method-receiver--self-and-self) — method receiver
> forms (`self`, `move self`, `Self`) apply to enums unchanged. [§19](#19-option-and-checked-types-and-variant-names)
> — `Option<T>` and `Checked<T, E>` are the canonical generic enum examples.
> [§21](#21-match--pattern-matching) — pattern matching on enum variants.
> [§22](#22-impl-block-syntax) — `impl` block structure and conflict detection.

---

## §21 Match / pattern matching

**Decided: `match expr { arms }`. Arms separated by `=>`, terminated by `,` (trailing
comma permitted). Arm bodies are expressions — braces required only for multi-statement
arms. Exhaustiveness on statically-enumerable types (enums) is a compile error.
`match` is itself an expression. Five pattern forms in v1: wildcard, literal, binding,
unit/tuple variant. Or-patterns and guards included. Range patterns, `@`-binding,
struct patterns, and slice patterns deferred.**

```ofn
# Basic Option match — match is an expression
let label = match opt {
    Some(x) => format_value(x),
    None    => "absent",
};

# Checked error handling — the only way to supply a fallback for Checked<T, E>
let val = match read_config("app.ofn") {
    Ok(cfg)  => cfg.timeout,
    Err(msg) => {
        log_error(msg);
        DEFAULT_TIMEOUT
    },
};

# Guard: extra boolean condition after the pattern
match score {
    Some(n) if n >= 90 => grade_a(n),
    Some(n) if n >= 70 => grade_b(n),
    Some(n)            => grade_c(n),
    None               => no_grade(),
};

# Or-pattern: one arm covers multiple variants
match direction {
    North | South => adjust_vertical(),
    East  | West  => adjust_horizontal(),
};
```

**Match form:**

`match subject { arms }` — no parentheses around the subject (consistent with
`if`/`while`/`for`). Outer braces are mandatory per §4. `match` is itself an expression:
the whole construct evaluates to the value produced by the matching arm. In statement
position, the expression statement ends with `;` per §3.

**Arm separator — `=>` (new token `Token::FatArrow`):**

All other separator tokens are allocated: `:` is type annotation (§9), `->` is return
type (§6), `=` is binding/assignment (§5, §10). `=>` is unallocated in Ofan and carries
no existing meaning. Added to the lexer as `Token::FatArrow`; scanned by peeking for `>`
after `=` (before the `==` path). Does not disturb `=`, `==`, or `>=`.

**Arm body — braceless expression canonical; braces for multi-statement:**

Single-expression body: no braces — this is the canonical persisted form. Multi-statement
body: braces required. `=>` and the trailing `,` make arm boundaries unambiguous regardless;
the `goto fail` class of bug (the rationale for §4's mandatory braces on control-flow
bodies) does not apply when a dedicated separator + terminator already mark the boundary.

```ofn
Some(x) => x + 1,          # single expression — no braces (canonical)
None    => {                # multi-statement — braces required
    log("absent");
    0
},
```

§4's "braces mandatory" rule applies to control-flow block bodies (`if`/`while`/`for`/
`loop`). Match arms are expression positions with explicit `=>` / `,` boundaries — a
categorically different syntactic position. No exception to §4; this is a different rule.

**Pillar-3 canonicalization (arm body braces):** the compiler accepts `pattern => { expr },`
where the block contains a single expression without a `;`. The formatter normalizes this
to `pattern => expr,`, removing the redundant braces. Persisted shared-source files
therefore always have braceless single-expression arms — only one form appears in checked-in
code. Writers may type either form; the formatter enforces one.

**Arm terminator — comma, trailing permitted:**

Every arm body is followed by `,`, including multi-statement block arms (`{ ... },`).
Trailing comma after the last arm is permitted (same policy as enum variants §20 and
struct fields §10 — consistent, reduces diff noise).

**Pattern forms:**

*Wildcard* — `_`. Matches anything, binds nothing. `_` lexes as `Token::Ident("_")`;
the parser special-cases the bare underscore. Any identifier starting with `_` followed
by further characters (e.g. `_unused`) is a normal binding pattern, not a wildcard.

*Literal* — integer, float, bool, char, or string literal. Exact-value match:

```ofn
match code {
    0    => "ok",
    404  => "not found",
    _    => "other",
}
match flag {
    true  => on(),
    false => off(),
}
```

*Binding* — a bare identifier in pattern position that the type-checker does not resolve
as a variant name. Binds the matched value to that name in the arm body scope.

*Unit variant* — a bare identifier that the type-checker resolves as a known unit variant
of the match subject's enum type. Matching is exhaustiveness-tracked.

*Tuple variant* — variant name followed by `(` comma-separated sub-patterns `)`.
Sub-patterns are themselves full patterns; nesting (`Some(Some(x))`) works without
depth limit.

```ofn
match opt_pair {
    Some(Some(x)) => x,
    Some(None)    => default(),
    None          => fallback(),
}
```

**Binding vs. variant disambiguation — type-resolved (consequence of §2):**

§2 decided no compiler-enforced casing rule. The standard ML/Rust heuristic
(uppercase-first = variant, lowercase = binding) is therefore unavailable. Instead:

1. Parser emits all bare identifiers in pattern position as ambiguous name nodes.
2. Type-checker resolves: if the identifier names a variant of the match subject's enum
   type → variant match; otherwise → binding.
3. Pillar-5 warning when a bare identifier in pattern position does not match any variant
   of the subject type *and* an enum in scope has a variant spelled identically (wrong
   type, same spelling): "identifier `X` does not match any variant of `TypeName` — this
   arm binds the entire value to `X`. Did you mean a different match subject type?"

Tuple variant patterns (`Some(x)`, `Ok(val)`) are unambiguous at parse time — the
parenthesized sub-pattern list can only belong to a constructor application, never a
binding. The disambiguation issue affects only bare identifiers (unit variant vs. binding).

**Unreachable arms — compile error (pillar 1):** a binding arm (bare identifier or `_`)
that makes all subsequent arms unreachable is a compile error:

```
error: unreachable arm — the binding `val` matches every value of `Option<i32>`
  arm at line 4 can never be reached
suggestion: move the binding arm after the more-specific variant arms, or remove it
```

This closes the silent-logic-bug surface created by the type-resolved disambiguation:
a programmer who mistypes a variant name gets a binding that catches everything — the
unreachable-arm error on the arms below it makes the mistake visible rather than silent.

**Or-pattern:**

`|` separates alternatives inside a single arm:

```ofn
match dir {
    North | South => vertical(),
    East  | West  => horizontal(),
}
```

`Token::Pipe` is already in the lexer. In match arm pattern position the parser context
makes pattern-`|` vs. bitwise-OR unambiguous (patterns cannot contain arbitrary
expressions). Both sides of an or-pattern must bind the same set of names with the same
types — a compile error otherwise:

```
error: or-pattern arms bind different names
  left arm binds: x: i32
  right arm binds: y: i32
suggestion: use the same binding name on both sides, or use `_` to discard the payload
```

**Pillar-3 note — leading `|`:** a leading `|` before the first alternative (`| North | South`)
is accepted by the parser as a write-time convenience (alignment aid). The formatter removes
it; it never persists in shared-source files. Canonical form has no leading `|`.

**Guard:**

`if condition` between the pattern and `=>`. Evaluated only after the pattern matches.
A guarded arm does **not** count as covering its pattern for exhaustiveness — the
compiler treats a guarded arm as a partial cover requiring other arms to complete
coverage of the matched variants:

```ofn
match opt {
    Some(x) if x > 0 => positive(x),
    Some(x)          => non_positive(x),  # required: guard above is partial cover of Some
    None             => absent(),
}
```

**Exhaustiveness — compile error (pillar 1):**

Non-exhaustive match on any enum is a **compile error** (not a warning, not runtime
panic). Enum variants are always statically enumerable; there is no execution path in
which a missed variant is acceptable. Error message quality (pillar 5):

```
error: non-exhaustive match — missing variants for `Direction`:
  · South
  · East
  · West
suggestion: add arms for the missing variants, or add `_ => ...` to catch all remaining
```

For non-enumerable types (integers, strings, floats), a `_` wildcard arm or a set of
guard-free literals that provably covers all values is required — omitting it is also
a compile error:

```
error: non-exhaustive match on `i32` — open-ended type requires a catch-all arm
suggestion: add `_ => ...` as the final arm
```

**`match` on `Checked<T, E>` — the only fallback path (§12, §19):**

`?:` is deliberately invalid on `Checked<T, E>` (§12). `match` is the only mechanism
for branching on the failure arm and supplying a fallback value. The exhaustiveness rule
means both `Ok` and `Err` arms must always be present — no silent discard of errors.

**Deferred:**

- **Range patterns** (`0..10 =>`) — range literal/expression syntax not yet decided (§24).
- **`@`-binding** (`x @ Some(y) =>`) — binds the whole matched value and destructures;
  useful for logging/re-wrapping but not essential for `Option`/`Checked` use cases.
- **Struct patterns** (`Rect { width, height } =>`) — struct variants are deferred in
  §20; struct patterns follow when struct variants are decided.
- **Slice/array patterns** — array/slice literal syntax is §24 deferred.
- **Or-pattern exhaustiveness with guards** — the rule for when `A | B` in a guarded
  arm counts toward exhaustiveness is subtle; defer to type-checker design session.

> **See also:** [§3](#3-statement-termination) — `;` after a `match` expression in
> statement position. [§4](#4-block-delimiters) — outer braces mandatory; arm-body
> braces required only for multi-statement arms (different rule, see rationale above).
> [§12](#12-operators) — `?` and `?:` operators; `?:` is explicitly invalid on
> `Checked<T, E>`, making `match` the sole fallback mechanism. [§19](#19-option-and-checked-types-and-variant-names)
> — `Option<T>` and `Checked<T, E>` variant names; `match` is the mandated inspection
> mechanism for `Checked`. [§20](#20-enum-declaration-syntax) — enum variant forms
> (unit and tuple) that patterns destructure.

---

## §22 `impl` block syntax

**Decided: `impl TypeName { ... }` blocks attach methods and associated functions to a named
type. Multiple `impl` blocks for the same type are permitted anywhere in the program; the
compiler merges them into one namespace. Duplicate method or associated-function names across
any blocks are a hard compile error citing all conflict sites.**

```ofn
impl Entity {
    fn distance_to(self, other: &Entity) -> f32 { ... }  // method — §18 receiver
    fn update(self, dt: f32) { ... }                      // mutable borrow inferred
    fn into_id(move self) -> u32 { ... }                  // consuming method
    fn default() -> Self { Entity { ... } }               // associated function — no receiver
}
```

### Structure

`impl TypeName { items }` — braces mandatory per §4. Items are `fn` declarations (§6). Two
kinds:

- **Method:** first parameter is a `self` or `move self` receiver per §18. Access mode is
  inferred from body usage; `move self` is the only explicit override.
- **Associated function:** no receiver parameter. Called without an instance. `Self` in
  return type or body resolves to `TypeName`.

An `impl` block is a declaration namespace, not an executable block. No expressions, `let`
bindings, or free statements are valid at the top level — only `fn` declarations.

### Multiplicity

Any file may contain any number of `impl TypeName` blocks for the same type. The compiler
merges all of them across the whole program into one method/associated-function namespace.
This is a **whole-program property**, consistent with the single-binary-install model
(pillar 4) and the compiler's existing whole-program analysis.

```ofn
// file: entity_movement.ofn
impl Entity {
    fn move_by(self, dx: f32, dy: f32) { ... }
}

// file: entity_render.ofn
impl Entity {
    fn draw(self, canvas: &Canvas) { ... }
}
// Both blocks merge: Entity has both move_by and draw.
```

No declaration is needed to "open" a type for extension. Any `impl TypeName` block is valid
wherever `TypeName` is in scope.

### Conflict detection

Duplicate method or associated-function names for the same type, across any combination of
`impl` blocks and files, are a **hard compile error** (pillar 1). The error cites **all**
conflict sites by file and line (pillar 5) — no silent last-write-wins.

```
error: duplicate method `draw` on type `Entity`
  → entity_render.ofn:4:5 — first definition
  → entity_render_hd.ofn:12:5 — duplicate definition
note: all `impl Entity` blocks merge into one namespace;
      each name must be unique across all of them
suggestion: rename one of the conflicting definitions
```

### Pillar-alignment rationale

**Pillar 1 (explicit erroneous behavior):** merge makes the namespace global to the type;
any ambiguity is a compile error, not a resolution tie-break. Citing all conflict sites
(not just the duplicate) ensures the programmer can locate and resolve the conflict without
guessing which file "won."

**Pillar 3 (single canonical syntax):** one block syntax, one keyword (`impl`), one item
kind inside (`fn`). No `extend`, no `open impl`, no type prefix on individual methods. The
block itself carries the type binding.

**Pillar 4 (single-binary install):** merge-at-compile-time requires no separate linking
step. The compiler sees all source; the merged namespace assembles once in a single
whole-program pass — the same analysis model already used for the existing declaration
collection pass in the type-checker.

**Pillar 5 (context + suggestion in errors):** the duplicate-name error must explain the
merge rule. "Duplicate definition" without it is pedagogically opaque for a programmer who
wrote two `impl Entity` blocks in different files without knowing they share a namespace.

### Deferred (§24)

`impl Trait for Type` — trait implementation syntax is not decided here and remains in §24.
The block-merging mechanism above is designed to generalize: a trait impl is structurally
"another `impl` block for the same type, scoped to a named trait's method set." No rework
of the merge or conflict-detection rules is anticipated when trait impls are added. This
ordering — `impl` block structure settled first, trait impls as an additive extension — is
a deliberate design choice.

*Pre-existing pillar 1 gap (noted, not fixed here):* the type-checker's current
`collect_fn_sig` pass (`src/typechecker/infer/mod.rs:56`) uses a bare `HashMap::insert`
for free-function signatures, which silently overwrites on duplicate names. Two top-level
`fn foo()` definitions will not produce a compile error today. Fixing this and implementing
impl-block conflict detection belong in the same future session — they should share one
declaration-collection pass rather than being patched independently.

> **See also:** [§6](#6-function-declarations) — `fn` syntax used inside `impl` blocks.
> [§18](#18-method-receiver--self-and-self) — `self`, `move self`, `Self` receiver forms.
> [§24](#24-not-yet-decided--deferred) — `impl Trait for Type` syntax deferred.

---

## §23 Struct field access

**Decided: `obj.field` reuses the existing dot operator — no new token, no new grammar rule.
Access mode (immutable or mutable borrow) is inferred from body usage by the same mechanism as
§18 `self` receiver inference and §17 Copy/Move inference. Copy-typed fields read by value via
implicit-Copy per §17. Non-Copy field ownership beyond a borrow is a hard compile error at
phase 1 — partial-move tracking is explicitly deferred. Mutation through a shared reference is a
hard compile error (same shape as the `ConsumeViaRef` check from PR #27). Visibility is deferred.
No `move obj.field` syntax until a dedicated phase-2 design session.**

### 1. Syntax

`obj.field` uses the dot operator already in the lexer and parser. Field access vs. method call
is disambiguated by trailing `()`:

```ofn
let x = point.x;        // field access — no ()
let d = point.dist();   // method call  — ()
```

No new token or grammar rule is required. `Expr::Field` is already in the AST
(`src/ast/expr.rs:48`) and parsed in `parse_postfix` (`src/parser/expr.rs:116`). This section
formalizes what the implementation already does.

### 2. Access mode — inferred, not annotated

`obj.field` always yields a borrow of the field. The borrow mode — immutable or mutable — is
inferred from how the field is used in the enclosing body:

- **Immutable** if the field is only read (value used, passed to a shared-reference parameter,
  or compared).
- **Mutable** if the field is assigned to (`obj.field = value`) or passed to a mutable-borrow
  parameter.

This is the **exact same inference mechanism** already built for `self` in §18 (receiver access
mode inferred from body usage) and the Copy/Move structural inference in §17. No new inference
concept is introduced. The programmer does not write `&obj.field` or `&mut obj.field` in source;
the mode is compiler-visible information only. Per pillar 3, there is one way to access a field
in shared source: bare `obj.field`.

### 3. Copy fields — implicit copy by value

If the field's type is inferred as `Copy` by §17's rule (all primitives; structs/enums whose
every field/payload is recursively `Copy`), then reading `obj.field` in a value position is an
implicit copy of the field's value — the same implicit-Copy inference already used everywhere
else in the language. No syntax change at the call site; no keyword.

### 4. Non-Copy field ownership — phase 1 boundary

If the body's usage of `obj.field` genuinely requires **ownership** (not just a borrow) and the
field's type is not `Copy`, this is a **hard compile error** — not silent, not a partial move,
not a deferred panic:

```
error: cannot move `Entity::sprite` out of a field access — partial moves are not supported yet
note: moving a single field out of a struct requires tracking that the struct is partially moved,
      which is not implemented in this compiler phase
suggestion: either move the whole struct (pass `entity` instead of `entity.sprite`),
            or restructure the code so `sprite` is accessed only by borrow
```

**Partial-move tracking is explicitly deferred to phase 2 / borrow-checker work.** Partial-move
tracking requires lifetime/region machinery that does not yet exist. This mirrors the pattern
established by the §18 `SelfAccessAmbiguity` error and the `ConsumeViaRef` error (PR #27): when
the compiler cannot yet prove something safe, it says so with full context and actionable
alternatives — it does not half-implement a check that could miss cases. Pillar 1 mandates the
compile error; pillar 5 mandates the message quality.

### 5. Mutation through a shared reference — hard error

Assigning to a field through a shared reference is a hard compile error:

```ofn
let r: &Entity = &entity;
r.x = 1.0;   // ERROR — cannot assign through a shared reference
```

```
error: cannot assign to `Entity::x` through a shared reference
note: `r` is a shared borrow (`&Entity`) — field mutation requires either a mutable borrow
      (`&mut Entity`) or an owned value
suggestion: use `&mut entity` if mutation is intended, or restructure so the owning
            binding `entity` is used directly
```

This is the **same ownership-violation shape** as `TypeError::ConsumeViaRef` from PR #27
(calling a `move self` method through a `&T` receiver). The typechecker check mirrors that
pattern: detect `Ty::Ref { mutable: false }` as the object type when the field appears in an
assignment lvalue position, emit a hard error, do not cascade. The two checks share a pattern,
not a code path — the error variant is field-access-specific.

### 6. Visibility — deferred

Access control (`pub`, `private`, crate-level visibility) is **not decided here**. For phase 1,
all fields are accessible anywhere within the compiling program. Visibility gates belong to the
module/import design session (§24), not here. No `pub` keyword on struct fields until that
session settles module syntax.

### 7. No `move obj.field` syntax — explicitly rejected for now

Pre-reserving `move obj.field` syntax ahead of the phase-2 partial-move design session is
**rejected for this session**, with the following rationale:

Internal compiler scaffolding added ahead of its use — `Ty::TyVar`, `Region::Var`, phase-2
`TypeError` variants — costs nothing observable: those identifiers live only in `.rs` source
files and have no surface in the compiled program or in persisted `.ofn` source files.

`move obj.field` is different in kind: it would be **user-facing syntax written into real
programs** before its semantics are designed. If the phase-2 partial-move design session settles
on different syntax — or settles that `move obj.field` is not the right mechanism at all — there
are two bad outcomes: either the language breaks existing programs that used the pre-shipped form,
or it supports two syntactic forms for the same operation, violating pillar 3 (single canonical
syntax in shared source). Neither outcome is acceptable.

The decision rule: no field-access-specific `move` syntax ships until a dedicated phase-2 design
session settles it spec-first, the same process used for every other construct in this language.

### Pillar-alignment rationale

**Pillar 1 (explicit erroneous behavior):** two hard errors, each with full context: non-Copy
ownership attempts cite the partial-move gap and give two escape paths; shared-ref mutation cites
the borrow kind and suggests `&mut`. Neither is silent, neither defers to runtime. The phase-1
error boundary is declared explicitly in the spec, not left to implementers to discover.

**Pillar 2 (lifetime inference with opt-in escape hatch):** access mode is inferred — the
programmer writes `obj.field` in all cases; the compiler determines immutable vs. mutable borrow
from usage. No annotation, no escape hatch yet (`move obj.field` is deferred pending phase-2
design). This is the same ergonomic contract as §18's `self` receiver.

**Pillar 3 (single canonical syntax):** one form — `obj.field` — for all field reads and writes
regardless of inferred access mode. No `&obj.field` or `&mut obj.field` in source. No
`move obj.field` until the phase-2 session designs it.

**Fifth validation of the §17/§18 model:** struct field access is the fifth syntactic position at
which the inference model applies — struct declaration (§17), function parameters (§7), method
receivers (§18), loop iteration (§16), and now field reads/writes. No special cases at any
position.

> **See also:** [§7](#7-lifetime--region-inference-and-escape-hatch) — borrow inference.
> [§17](#17-copymove-semantics) — Copy/Move rule applied to field types.
> [§18](#18-method-receiver--self-and-self) — same inference mechanism at the receiver position;
> `ConsumeViaRef` pattern this section's mutation-through-ref check mirrors.
> [§22](#22-impl-block-syntax) — impl block structure; methods that access `self` fields follow
> the same borrow inference as standalone field access.
> [§24](#24-not-yet-decided--deferred) — visibility/module syntax; partial-move tracking.

---

## §24 Not yet decided — deferred

The following constructs have appeared informally in design examples, or were surfaced
during review, but have **never been formally decided**. Do not assume any particular
syntax is settled for these.

**Lexer-relevant (deferred deliberately, not overlooked):**
- **Unicode escapes** (`\u{...}`-style) — see §15
- **Raw strings** (no escape processing) — see §15

**Parser/typechecker-relevant (out of scope for the lexer's first pass; do not block it):**
- **Trait / interface syntax** — not started; how `impl` blocks interact with named
  traits has not been decided, though the receiver forms themselves are now settled in §18
- **Module / import syntax and path separator** — `::` used informally in examples only
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

**Reserved words — master list (updated 2026-06-29):** the following words are reserved
in the lexer (`src/lexer/keywords.rs`) ahead of their syntax being decided. Reservation
means they cannot be used as identifiers; it does **not** imply any grammar or semantics
has been decided for them.

Words reserved from **decided syntax** (§16, §17, §18, §21) that were not yet in the keyword table:

| Word | Token | Source |
|------|-------|--------|
| `loop` | `Token::Loop` | §16 loop syntax |
| `copy` | `Token::Copy` | §17 Copy/Move modifier |
| `move` | `Token::Move` | §17 Copy/Move modifier |
| `self` | `Token::SelfKw` | §18 method receiver value |
| `Self` | (type name, not a token variant) | §18 impl-block type alias; resolves via type namespace |
| `impl` | `Token::Impl` | §22 impl block syntax |
| `match` | `Token::Match` | §21 match / pattern matching |

Words reserved **ahead of syntax decisions** (constructs in this §24 list):

| Word | Token | Future construct |
|------|-------|-----------------|
| `trait` | `Token::Trait` | trait / interface syntax (§24) |
| `mod` | `Token::Mod` | module / import syntax (§24) |

**Process note, not a syntax item:** the coordination gap flagged here (no master reserved-
word list) is now resolved by the table above.

These do not block lexer work on the tokens decided in §1–§23, but the token set will
need a follow-up pass once the parser/typechecker-relevant items above are resolved.

---

*Source: content migrated from `docs/prds/2026-06-26-lexer.md` during the 2026-06-26
documentation reorganization. Extended in a follow-up session the same day to resolve
§1, §2, and the §7/§13 sub-items, and to add §14 (numeric literals) and §15 (string/char
literals).*