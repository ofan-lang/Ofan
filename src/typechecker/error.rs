use crate::lexer::token::Span;
use crate::typechecker::ty::Ty;
use thiserror::Error;

/// All type errors the typechecker can emit.
///
/// Follows the same `span + context + suggestion` pattern as `ParseError`
/// (pillar 5: every error includes location and an actionable suggestion).
///
/// Phase 2 variants are present now — adding them later would be a breaking
/// change for any exhaustive `match` on `TypeError` in caller code.
#[derive(Debug, Error)]
pub enum TypeError {
    // ── Phase 1 errors ────────────────────────────────────────────────────────

    #[error("type mismatch at byte {}: expected {expected:?}, found {found:?}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    Mismatch {
        expected: Ty,
        found: Ty,
        span: Span,
        suggestion: Option<String>,
    },

    #[error("undefined variable `{name}` at byte {}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    UndefinedVariable {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    #[error("undefined function `{name}` at byte {}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    UndefinedFunction {
        name: String,
        span: Span,
        suggestion: Option<String>,
    },

    #[error("wrong number of arguments for `{name}` at byte {}: expected {expected}, found {found}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    ArgCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
        suggestion: Option<String>,
    },

    #[error("condition must be `bool`, found {found:?} at byte {}\
        \n  suggestion: use a comparison expression to produce a `bool`", span.start)]
    NonBoolCondition { found: Ty, span: Span },

    #[error("if-else branch type mismatch at byte {}: then-branch has type {then:?}, \
        else-branch has type {else_:?}\
        \n  suggestion: ensure both branches return the same type", span.start)]
    BranchMismatch { then: Ty, else_: Ty, span: Span },

    #[error("return type mismatch at byte {}: expected {expected:?}, found {found:?}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    ReturnMismatch {
        expected: Ty,
        found: Ty,
        span: Span,
        suggestion: Option<String>,
    },

    /// Non-fatal: a syntactically valid construct that phase 1 does not yet
    /// type-check. Inference continues with `Ty::Error` for the node.
    /// Does not count as a hard failure in `Result::Err`.
    /// Surfaced in `InferResult::deferred` so callers (driver, codegen) know
    /// which constructs were accepted without full verification.
    #[error("type checking not yet implemented for `{feature}` at byte {} \
        — this construct is accepted but not fully verified; \
        avoid lowering it to code until the next compiler phase adds support",
        span.start)]
    Deferred { feature: &'static str, span: Span },

    // ── Phase 2 placeholders ──────────────────────────────────────────────────
    // These variants are never constructed in phase 1 but must be in the enum
    // now to avoid a breaking change when phase 2 adds borrow/lifetime checking.

    /// Lifetime/region conflict — emitted when region constraint solving fails.
    /// Phase 2.
    #[error("lifetime conflict at byte {}: {note}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    LifetimeConflict {
        span: Span,
        note: String,
        suggestion: Option<String>,
    },

    /// Use of a moved value. Phase 2.
    #[error("use of moved value `{name}` at byte {}; value was moved at byte {}{}", use_span.start, moved_at.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    UseAfterMove {
        name: String,
        use_span: Span,
        moved_at: Span,
        suggestion: Option<String>,
    },

    /// Conflicting borrows. Phase 2.
    #[error("conflicting borrows at byte {}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    BorrowConflict {
        span: Span,
        suggestion: Option<String>,
    },
}

impl TypeError {
    /// True for errors that halt the inference result.
    /// `Deferred` is non-fatal — inference continues and the caller gets `Ok`.
    pub(crate) fn is_fatal(&self) -> bool {
        !matches!(self, TypeError::Deferred { .. })
    }
}
