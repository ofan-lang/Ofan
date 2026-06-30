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
| [17](#17-copymove-semantics) | Copy/Move semantics | Decided |
| [18](#18-method-receiver-syntax) | Method receiver syntax | Decided |
| [19](#19-not-yet-decided--deferred) | Not yet decided — deferred | — |

---

## Status summary

At a glance: **17 of 18 numbered sections decided** (§15 has two narrow, deliberately
deferred extensions — Unicode escapes and raw strings — that do not block the core
lexer work). Every token-level construct needed for a first lexer implementation is now
covered. The remaining open ground is §19 — constructs that have never been formally
designed at all (loops, `match`, traits, modules, enums, attributes, array literals,
generic call syntax, void/unit type) — which were always out of scope for the lexer's
first pass and do not block it.

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

> **See also:** [§18](#18-method-receiver-syntax) — Method receiver syntax
> (`&self`/`&mut self`/`self`) builds directly on this section's Copy/Move rule for its
> consuming-receiver case; see §18 for the full decision.

---

## §18 Method receiver syntax

**Decided: three receiver forms — `&self` (immutable borrow), `&mut self` (mutable borrow),
and `self` (consuming) — reusing the borrow and mutability mechanisms already locked in §5
and §7, with no new syntax concepts introduced at the receiver position.**

```ofn
impl Entity {
    fn distance_to(&self, other: &Entity) -> f32 {
        // read-only — does not mutate self, self remains usable by the caller
        // after this call
    }

    fn update(&mut self, dt: f32) {
        self.x = self.x + self.velocity_x * dt;
        // mutates self in place — self remains the SAME binding, still usable
        // by the caller after this call (this is the form that makes the
        // call-update-every-frame-in-a-loop pattern from the game-dev example
        // actually work)
    }

    fn into_id(self) -> u32 {
        self.id
        // consumes self — behavior depends on whether Entity is Copy or Move
        // per the §17 rule: if Move, the caller's original binding becomes
        // invalid after this call; if Copy, the caller's original is
        // untouched (a duplicate was handed to the method)
    }
}
```

**The three forms:**

`&self` — immutable borrow of the receiver. The method may read fields but may not mutate
them. The caller's binding is fully usable and unchanged after the call. This is the form
for any operation that only inspects state — queries, computations, serialization,
comparisons.

`&mut self` — mutable borrow of the receiver. The method may read and mutate fields in
place. The caller's binding is the same binding, still usable after the call — only its
contents may have changed. This is the form for in-place state updates: `update`, `push`,
`set_*`, any operation that modifies the receiver and leaves it intact for continued use.

`self` (bare, consuming) — the receiver is passed by value. Whether the caller's original
binding survives is fully determined by §17's Copy/Move rule: for a `Move` struct, the
caller's binding becomes invalid after the call (ownership transferred into the method);
for a `Copy` struct, the caller's binding is untouched (a duplicate was handed to the
method, consistent with Copy semantics everywhere else in the language). Use this form for
transforming a value into something else — `into_*` conversions, destructuring, consuming
builders.

> **Note — `mut self` vs. `&mut self` (these are not the same thing):** a receiver written
> as `mut self` (no `&`) is a *consuming* receiver in the same category as bare `self`,
> where the locally-owned copy happens to be declared mutable within the method body. It
> does **not** mean "mutate the caller's original in place" — that is `&mut self`. The
> one-character difference (`&`) carries real semantic weight, the same weight that
> separates `let x` from `let mut x` in §5. A reader skimming a method signature reaches
> the crucial information at the `&` character: `&` present means the caller keeps the
> binding; `&` absent means the call consumes it. Per pillar 5, this distinction must be
> explicit and scannable at the signature, not something a reader has to infer from the
> method body.

*Rationale (pillars 1 and 2, and §17 validation):*

**Pillar 2:** all three forms reuse syntax already locked in this spec. `&` and `&mut` are
the borrow mechanisms from §7; `mut` as a modifier is from §5; the consuming case applies
the Copy/Move rule from §17 with no additional mechanism. A method receiver is, in terms of
the language's type system, simply a parameter named `self` — it follows the same rules as
any other parameter at a function boundary. Nothing new is introduced at the receiver
position; all of the relevant behavior is already specified elsewhere and generalizes to
this case without special-casing.

**Pillar 1:** `&mut self` existing as its own explicit form closes a real silent-danger gap.
Without it, a mutating method would have to be expressed either as a consuming `self`
receiver (wrong semantics: needlessly invalidates or needlessly duplicates the caller's
binding depending on Copy/Move status) or as some form of implicit mutation with no marker
at the signature level, which would be exactly the kind of unmarked dangerous behavior
pillar 1 forbids. The three-way split is not convention-following for its own sake — it is
the minimum needed to make the dangerous case (mutation of the caller's state) visible at
the signature without forcing the common case (read-only inspection) to pay ceremony.

**§17 validation:** the consuming-receiver case (`self`) is a direct confirmation that
§17's design generalizes correctly. The same Copy/Move rule that governs ordinary function
parameters governs `self` — no "method exception" to the ownership model, no separate
concept to learn for method calls vs. free function calls. This is the clean generalization
that §17's design was expected to produce.

> **See also:** [§17](#17-copymove-semantics) — Copy/Move semantics determine the
> consuming-receiver behavior for `self` parameters. [§19](#19-not-yet-decided--deferred) —
> trait/interface syntax (how `impl` blocks interact with named traits) remains unresolved
> and does not block the method-receiver decision here.

---

## §19 Not yet decided — deferred

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
- **Trait / interface syntax** — not started; how `impl` blocks interact with named
  traits has not been decided, though the receiver forms themselves are now settled in §18
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

**Reserved words — master list (resolved 2026-06-29):** the following words are reserved
in the lexer (`src/lexer/keywords.rs`) ahead of their syntax being decided. Reservation
means they cannot be used as identifiers; it does **not** imply any grammar or semantics
has been decided for them.

Words reserved from **decided syntax** (§17, §18) that were not yet in the keyword table:

| Word | Token | Source |
|------|-------|--------|
| `copy` | `Token::Copy` | §17 Copy/Move modifier |
| `move` | `Token::Move` | §17 Copy/Move modifier |
| `self` | `Token::SelfKw` | §18 method receiver value |
| `impl` | `Token::Impl` | §18 impl blocks |

Words reserved **ahead of syntax decisions** (constructs in this §19 list):

| Word | Token | Future construct |
|------|-------|-----------------|
| `loop` | `Token::Loop` | loop syntax (this section) |
| `match` | `Token::Match` | pattern matching (this section) |
| `trait` | `Token::Trait` | trait / interface syntax (this section) |
| `mod` | `Token::Mod` | module / import syntax (this section) |

`Self` (capital) is **not** reserved — whether Ofan needs a distinct `Self` type alias
inside `impl` blocks has not been decided and is a §19-adjacent open question.

**Process note, not a syntax item:** the coordination gap flagged here (no master reserved-
word list) is now resolved by the table above.

These do not block lexer work on the tokens that *are* decided in §1–§15, but the token
set will need a follow-up pass once the parser/typechecker-relevant items above are
resolved.

---

*Source: content migrated from `docs/prds/2026-06-26-lexer.md` during the 2026-06-26
documentation reorganization. Extended in a follow-up session the same day to resolve
§1, §2, and the §7/§13 sub-items, and to add §14 (numeric literals) and §15 (string/char
literals).*