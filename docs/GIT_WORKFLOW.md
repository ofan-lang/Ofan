# Git workflow — Ofan

## Commit message format

Conventional Commits: `<type>(<scope>): <subject>`

**Types:**

- `feat` — new language feature or compiler capability
- `fix` — bug fix in the compiler
- `docs` — documentation only
- `chore` — tooling, config, CI, dependencies (no production code change)
- `refactor` — internal restructuring without behavior change
- `test` — test additions or corrections

**Scope** is optional but encouraged for compiler subsystems: `lexer`, `parser`, `ast`,
`typechecker`, `codegen`.

**Subject:** imperative mood, lowercase, no trailing period, ≤50 chars.

**Body** (optional): explain the *why*, not the *what*. Wrap at 72 chars.

Examples:
```
chore: initial project scaffold — docs and Claude Code config
docs(philosophy): record Rust + LLVM stack decision and rationale
feat(lexer): implement tokenizer for numeric literals
fix(parser): handle missing semicolon without consuming next token
```

## Branching model

**Phase 1 — now, pre-`src/`:** direct-to-main. No feature branches.
The repo has no code to break and no CI to enforce. Overhead with no payoff.

**Phase 2 — once `src/` exists and CI is running:** feature branches for anything touching
`lexer/`, `parser/`, `typechecker/`, or `codegen/`. Pattern: `feat/<short-name>`.
Direct-to-main remains acceptable for `docs:` and `chore:` changes.
**Trigger to enter Phase 2:** the first commit to `src/`.

**Phase 3 — once the compiler can compile real Ofan programs:** branch protection on main,
CI required to pass before merge.

## Claude Code git permissions — what is intentionally absent

`.claude/settings.json` allowlists `git status`, `git log`, `git diff`, `git show` — read-only operations only.

`git commit` and `git push` are **deliberately not allowlisted**.

**Why omission, not convention:** a permission prompt is enforced by the tool regardless of
session state; a documented convention can be forgotten mid-session. These operations are
irreversible and visible outside the local repo — they warrant a friction point every time,
independent of what was discussed earlier in a session.

Do not add `git commit*` or `git push*` to the allowlist. Require explicit in-chat
confirmation before Claude Code runs either command.

## PR conventions

**Phase 1 (now):** No PRs. Direct-to-main. Before committing anything non-trivial, run
`pillars-reviewer` and `rust-idiom-reviewer` manually (per `CLAUDE.md` workflow).

**Phase 2 (feature branches):** PRs for all changes to compiler internals. Before opening:
1. Run `pillars-reviewer` on the branch diff.
2. Run `rust-idiom-reviewer` on the branch diff.
3. Write a PR description that explains *why* — it becomes the permanent record when
   `git log` is read months later.

PRs are worth it solo once real code exists: the description forces a moment of "would I
be comfortable explaining this decision to a future contributor?" before it lands.
Pre-code, they are overhead with no payoff — not used in Phase 1.

### Merge method

Always use **"Create a merge commit"** on GitHub — not "Squash and merge" or
"Rebase and merge".

Reason: merge commits preserve branch structure and per-commit history within a PR. As
the project grows beyond solo contribution, this makes it possible to trace which PR
introduced a given change (`git log --merges`, `git log --first-parent`) without
reconstructing history from squashed blobs.

This applies even to single-commit PRs (like PR #1) for consistency. In that specific
case squash would produce an identical result, but one unconditional rule is better than
a rule with exceptions.

**Repo settings:** If GitHub currently allows all three merge methods, restrict the repo
to merge commits only so the convention is enforced by the UI rather than just documented.
Apply manually: GitHub → repo Settings → General → Pull Requests → uncheck "Allow squash
merging" and "Allow rebase merging", leave only "Allow merge commits" checked.
