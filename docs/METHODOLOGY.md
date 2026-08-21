# Methodology — Ofan

## Feature development lifecycle

Every non-trivial change follows this sequence:

1. **Plan mode first** — no code until the plan is approved. Use `EnterPlanMode` in Claude
   Code. For human contributors: write a short proposal in the PR description before opening
   a draft PR. Large changes that skip this step will be asked to restart.
2. **Small steps with checkpoints** — on large pieces (parser, type-checker, codegen),
   break work into slices with a named milestone at each stop. Example: "implement X, stop,
   show output." Do not open a PR that covers two independent slices.
3. **Agent reviews before commit** — any PR that touches files under `src/` must run both
   `pillars-reviewer` and `rust-idiom-reviewer` against the diff. This is a hard requirement,
   not a judgment call. See [GIT_WORKFLOW.md](GIT_WORKFLOW.md) §Mandatory agent reviews.
4. **Update PROGRESS.md before closing the session** — record what was done, what was
   decided and why, and what's next. The next session starts by reading this file.
5. **Never commit failing tests** — `cargo test` and `cargo clippy -- -D warnings` must
   both pass on the branch before any commit lands on main.

---

## Documentation ownership

Each doc owns one topic. It does not restate content owned by another doc — it links.

| Doc | Owns | Does not duplicate |
|-----|------|--------------------|
| `README.md` | First impression: what Ofan is, one code example, CLI surface, one-command build, link table to authoritative docs | Build details → CONTRIBUTING; design rationale → PHILOSOPHY/SYNTAX_SPEC; compiler internals → ARCHITECTURE |
| `CONTRIBUTING.md` | Setup prerequisites, build/test/fmt/lint commands, pointers to METHODOLOGY and GIT_WORKFLOW | Compiler layout → ARCHITECTURE; commit format → GIT_WORKFLOW; lifecycle → METHODOLOGY |
| `CLAUDE.md` | Agent-specific enforcement: terse pillar reminders (checklist), project commands, rules for invoking agent reviewers | Full pillar text → PHILOSOPHY; commit format → GIT_WORKFLOW; lifecycle → METHODOLOGY |
| `docs/METHODOLOGY.md` | Feature development lifecycle, doc ownership table, doc update-trigger rules | Git mechanics → GIT_WORKFLOW; pillar content → PHILOSOPHY |
| `docs/GIT_WORKFLOW.md` | Git mechanics: commit format, branch phases, PR conventions, merge method, mandatory review requirements, branch cleanup | Lifecycle → METHODOLOGY; design rationale → PHILOSOPHY |
| `docs/PHILOSOPHY.md` | Design thesis, 5 pillars (authoritative text), competitive landscape, technical decisions | Syntax → SYNTAX_SPEC; implementation → ARCHITECTURE |
| `docs/SYNTAX_SPEC.md` | Concrete syntax reference: token shapes, keywords, operators, literal rules, §24 canonical deferred list | Rationale → PHILOSOPHY; implementation → ARCHITECTURE |
| `docs/ARCHITECTURE.md` | Compiler implementation: phases, submodule layout, cross-cutting patterns, "not yet designed" list | Language design → PHILOSOPHY/SYNTAX_SPEC; workflow → METHODOLOGY/GIT_WORKFLOW |
| `docs/PROGRESS.md` | Living session log: what was done, decided, and what's next — updated every session | Settled decisions migrate to PHILOSOPHY/ARCHITECTURE/SYNTAX_SPEC once stable |
| `SECURITY.md` | Vulnerability reporting policy | Standalone |
| `CODE_OF_CONDUCT.md` | Community standards (Contributor Covenant) | Standalone |

**The anti-drift rule for README:** README links to authoritative docs rather than restating
their content. When README content duplicates a doc, the README version is the one to trim.

---

## Doc update trigger rules

These are not optional — they are part of the definition of "done" for a PR:

| What changed in the PR | Must also update |
|------------------------|-----------------|
| Branch/merge policy, PR conventions, required checks | `docs/GIT_WORKFLOW.md` in the same PR |
| Syntax decision settled or revised | `docs/SYNTAX_SPEC.md` in the same PR |
| Design pillar, semantic decision, or technical strategy | `docs/PHILOSOPHY.md` in the same PR |
| Compiler phase, submodule layout, or cross-cutting pattern | `docs/ARCHITECTURE.md` in the same PR |
| Any working session | `docs/PROGRESS.md` before closing |

Updating docs in a follow-up PR is not acceptable — it creates a window where code and docs
disagree, and follow-up PRs have a well-documented habit of never landing.

---

## Direct-to-main vs. branch + PR for doc-only changes

GIT_WORKFLOW.md allows `docs:` and `chore:` commits to go direct-to-main. This applies to:

- Typos and broken links
- Stale notes (e.g., a parenthetical about a file "not yet created" after the file exists)
- Formatting and reordering with no semantic change

**Branch + PR is required for doc-only changes that:**

- Alter a stated policy (e.g., change a trigger rule in this file)
- Add or remove a doc from the ownership table above
- Change who owns a topic (re-scope a doc's authority)
- Revise the branching model or CI requirements in GIT_WORKFLOW.md

The criterion is: would a future contributor read this and reach a different conclusion about
how to work? If yes, it warrants a PR description as a permanent record of why the policy
changed.
