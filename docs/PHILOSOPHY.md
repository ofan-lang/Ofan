# Ofan — Design Document

> Living draft. Every decision here should be able to answer "why?" with a technical reason, not an aesthetic one.

Language name: **Ofan**. File extension: **.ofn**. Mascot: **Ofy** — a diminutive of Ofan.
The language name keeps a serious/imposing register; the mascot's diminutive softens that same
name for informal use (stickers, social media, community), without losing the direct connection
to the language name.

Mascot character name: still pending, to be decided together with the artists.

- Naming note: `Galgal` (.gal) and `Seraph`/`Sarap` (.srp) were evaluated as alternatives. `Ophan`
  as-is was discarded due to an existing collision with an internal analytics system at The
  Guardian, already present on GitHub under that exact name. `Ofan` was chosen over `Galgal` for
  being a more imposing pronunciation; `Galgal` came across as a bit more playful.

## 1. Project thesis

A systems language with strong memory safety (on par with Rust) but a noticeably lower learning
curve, achieved through automatic lifetime inference instead of mandatory explicit annotation. It
targets the gap between "very safe but slow to compile and hard to read" (Rust) and "fast but with
manual safety" (Zig).

**The three founding complaints** (the reason this project exists):
1. C/C++ has undefined behavior (UB) that is invisible in the code — it fails silently, in
   unpredictable ways, and the compiler optimizes assuming it never happens.
2. Rust is hard to read and write because of how dense the ownership/lifetime annotations are
   across almost every data type.
3. Rust is cumbersome to install (heavy toolchain) compared to a single portable binary.

## 2. Design pillars (decisions already made)

### 2.1 — Explicit erroneous behavior, never silent undefined behavior
When something can fail:
- If it's detectable at compile time → compile error, with a clear message and exact location.
- If it's only detectable at runtime → an explicit, documented panic/error (never silent UB that
  the compiler could use to "optimize" in a surprising way).

Guiding principle: *"erroneous behavior," not "undefined behavior"* — terminology also used by the
Carbon team (Google) as a design goal.

### 2.2 — Lifetime inference with opt-in escape hatch
The programmer does not manually annotate `'a` unless there is genuine ambiguity that the compiler
detects and cannot resolve on its own. Explicit annotation is the exception, not the norm — this is
what directly reduces the syntactic density that makes Rust hard to read and write.

### 2.3 — Single canonical syntax in persisted source code
More than one alias or shortcut may exist at write-time (editor, LSP, snippets), but the shared
source file (what gets versioned, code-reviewed, and read by someone else) is always normalized to
a single canonical form. Never two valid forms coexisting as persisted ambiguity in the file.

### 2.4 — Single-binary install
No heavy external toolchain. The goal is to compete directly with Rust's installation friction,
which requires managing toolchain versions, versus a single, portable binary.

### 2.5 — Error messages as a product, with context and suggestions
No error message is ever just "expected X, found Y." It must always include enough context to
understand why it happened and, whenever possible, a concrete suggestion for how to fix it.

## 3. Known risks (self-diagnosed)

- A one-person project competing against funded teams — the only real defense is radical focus on
  a narrow niche, not greater ambition of scope.
- Without C/C++ interop from day one, the adoption cost for any real team is high.
- Without governance/decision documentation from the start, the project depends entirely on the
  continued availability of a single person.
- Estimated real competitive time window: ~2-3 years before Carbon (the best-funded competitor)
  reaches production maturity.

## 4. Competitive landscape (state as of this research, 2026)

### Summary by competitor

| Language | 2026 status | Core strength | Exploitable weakness |
|---|---|---|---|
| **C / C++** | Dominant, #3 on TIOBE (~8%) | Performance, 50-year ecosystem | Silent UB, no real modules, slow compilation via textual `#include` |
| **Rust** | Dropped from #13 to #16 on TIOBE; 72-82% developer admiration | Memory safety at compile time, no runtime overhead | Learning curve (explicit lifetimes, slow-to-compile monomorphization) |
| **Zig** | Approaching 1.0 (end of 2026); #39 TIOBE | Very fast compilation, transparent manual control, "portable assembler" | Manual memory safety — with code agents generating more volume, the risk of memory bugs with no safety net grows |
| **Carbon (Google)** | Experimental; 0.1 MVP no earlier than end of 2026, v1.0 after 2028 | Bidirectional C++ interop, led by a former LLVM technical lead, Google backing | Its own memory-safety roadmap is delayed to avoid breaking C++ interop — it doesn't yet solve the problem it promises |

### Positioning map

Two relevant axes: **compile-time memory safety** vs. **compilation speed / tooling simplicity**.

- C: fast, unsafe.
- C++: unsafe, slow to compile (templates, no real modules).
- Rust: very safe, slow to compile.
- Zig: fast, manual safety (no compile-time guarantees).
- Go: fast, but with a GC — doesn't give the fine-grained memory control this project is after.

**The "safe + fast to compile" quadrant is essentially empty.** No current competitor occupies it —
each one traded off one side to win the other.

### Why new languages do gain traction (pattern observed in Go, Rust, Zig)

None of the three entered competing "in general" against the incumbent. Shared pattern:

1. **Real pain from a well-resourced sponsor** (Google with slow C++ → Go; Mozilla with memory bugs
   in the Firefox engine → Rust).
2. **Narrow initial niche**, not general purpose from launch (Google's internal microservices; an
   isolated component of the Firefox engine — Servo).
3. **Gradual expansion** only after proving real value in that specific niche.

Zig follows the same pattern without a corporate sponsor: it gained traction via real products
built in the language itself (Tigerbeetle, a financial database; Bun, a JavaScript runtime; internal
adoption at Uber) — not via general-purpose promises.

### Direct implication for this project

As a project with no corporate sponsor, the entry strategy can't be "cover more use cases than the
competitors" (that game is lost on resources alone). The viable strategy is:

- Be your own "anchor use case": build something real and necessary in the language itself, rather
  than waiting for a third party to adopt it first.
- Compete in depth in a specific niche, not in general-purpose breadth.
- Take advantage of the ~2-3 year window before Carbon reaches production to validate the design
  with real users, even if it's a small group.
- Document governance and design decisions from day one so the project doesn't depend solely on
  the continued availability of one person.

### Relevant market signals (2026)

- A CISA (US) directive requires organizations to move away from memory-unsafe languages starting
  January 2026 — a tailwind for any language with strong memory guarantees.
- A DARPA program with over $50 million to automatically translate C into memory-safe alternatives.
- With the mass adoption of code agents, the case for compile-time memory guarantees (vs. manual
  review of a larger code volume) gets stronger — one analyst who used Zig professionally reports
  having migrated to Rust for this specific reason.

## 5. Technical decisions

### 5.1 — Resolved

**Implementation language: Rust**
Rationale: writing a memory-safety-focused compiler in a language with silent UB (C/C++)
would contradict pillar 2.1. Zig was evaluated but lacks mature LLVM bindings. Rust has
`inkwell` (safe LLVM bindings) and `melior` (MLIR) as production-ready options.

**Compilation backend: LLVM**
Rationale: near-free multi-platform reach (x86, ARM, RISC-V, WASM, microcontrollers)
without writing per-architecture codegen by hand. Cranelift was evaluated as a lighter
alternative but lacks the platform coverage and optimization maturity needed to compete
with C/C++ output quality. `inkwell` is the selected Rust binding.

**Launch niche and sequencing strategy**
The five originally-listed target domains (microcontrollers, speedcoding, web dev, app dev,
game dev) cluster into two groups:
- *Lean cluster*: microcontrollers, speedcoding, game dev — no/minimal runtime, direct
  performance focus, no mandatory OS/heap assumptions.
- *Rich cluster*: web dev, app dev — richer stdlib, OS/platform integration, heavier runtime
  assumptions.

Building the lean cluster first is the correct sequence: retrofitting a heavy runtime down
to bare-metal later is far harder than building lean and layering richness on top. The risk
of starting rich and trimming down is that hidden heap/OS assumptions bake into the language
before they can be avoided.

**Anchor project**: a small, real, useful CLI tool (speedcoding-shaped), built from day one
under the constraint of no mandatory OS/heap assumptions — even before microcontroller
support exists. This keeps the core honest systems territory from day one, not retrofitted
later. This mirrors the Zig/Tigerbeetle and Rust/Servo patterns: prove value in a real
product before claiming general-purpose status.

Web and app development are explicitly deferred — sequenced later, not abandoned.

**C/C++ interop — v1 scope**
- *Direction*: calling INTO existing C code only (drivers, SDKs). Being called FROM C/C++
  (embedding Ofan as a library with a stable ABI) is out of scope for v1.
- *Language scope*: C only. C++ is reachable only via C-compatible shim layers — the
  standard approach most C++ libraries already expose. No native C++ ABI / template /
  exception support planned.
- *Mechanism*: explicit `extern` block with hand-written or tool-generated bindings
  (Rust-style), NOT direct C-header parsing (Zig-style `@cImport`-equivalent). Rationale:
  (a) keeps the FFI boundary explicit and auditable in source — a visible `extern` block can
  be grepped and reviewed, consistent with pillar 2.1 (no silent behavior); unlike an opaque
  header import, there is no ambiguity about what is being imported or what ABI is assumed;
  (b) avoids building a second parser (for C's grammar including preprocessor macros) before
  Ofan's own lexer/parser exist. A `@c_import`-equivalent ergonomic wrapper is noted as a
  possible future goal once the core compiler is mature — not a v1 commitment.

### 5.2 — Still pending

_(None at this stage.)_
