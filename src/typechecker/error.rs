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

    /// Consuming method called through a reference receiver — cannot move out of a borrow.
    /// Detected at the type level (no lifetime machinery needed): receiver is `Ty::Ref`
    /// but the method declares `move self`, requiring ownership.
    #[error("cannot call consuming method `{type_name}::{method_name}` on a reference receiver at byte {}\n\
        note: `{method_name}` declares `move self`, which requires ownership of `{type_name}`, \
        but the receiver is a reference and does not hold ownership\n\
        suggestion: call this method on an owned `{type_name}` value, \
        or change `move self` to `self` in the method signature if ownership is not required",
        span.start)]
    ConsumeViaRef {
        type_name: String,
        method_name: String,
        span: Span,
    },

    /// Method name not found in the impl block for the receiver's type (§22).
    /// Emitted for `obj.method()` when `impl_sigs[type]` exists but has no entry for `method`,
    /// or when the receiver type has no impl block at all.
    #[error("method `{method_name}` not found on type `{type_name}` at byte {}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    MethodNotFound {
        type_name: String,
        method_name: String,
        span: Span,
        suggestion: Option<String>,
    },

    /// §18 ambiguity: body of a bare-`self` method has both a consuming use of `self` and a
    /// non-consuming use. The compiler cannot infer a single access mode; `move self` should
    /// be written explicitly if consuming ownership is intended.
    #[error("cannot infer access mode for `self` in `{fn_name}` — \
        consuming use at byte {} conflicts with non-consuming use at byte {}\n\
        note: these requirements conflict — the method cannot simultaneously borrow and consume\n\
        suggestion: if consuming ownership is intended, write `move self` and restructure \
        the body so any borrowing use precedes the move",
        consuming_span.start, other_span.start)]
    SelfAccessAmbiguity {
        fn_name: String,
        consuming_span: Span,
        other_span: Span,
    },

    /// Duplicate top-level function name — same name declared twice in program scope.
    /// Both definition sites are cited; the first definition wins for subsequent checking.
    #[error("duplicate function `{name}` — first definition at byte {}, \
        duplicate at byte {} \
        — rename one of the conflicting definitions",
        first_span.start, duplicate_span.start)]
    DuplicateFn {
        name: String,
        first_span: Span,
        duplicate_span: Span,
    },

    /// Duplicate method or associated-function name within the merged impl namespace
    /// for a given type (§22 — all impl blocks for the same type form one namespace).
    #[error("duplicate method `{method_name}` on type `{type_name}` — \
        first definition at byte {}, duplicate at byte {}\n\
        note: all `impl {type_name}` blocks merge into one namespace; \
        each name must be unique across all of them\n\
        suggestion: rename one of the conflicting definitions",
        first_span.start, duplicate_span.start)]
    DuplicateMethod {
        type_name: String,
        method_name: String,
        first_span: Span,
        duplicate_span: Span,
    },

    /// Duplicate struct name — two `struct Foo { ... }` blocks with the same name.
    /// Both definition sites are cited; the first definition wins for subsequent checking.
    #[error("duplicate struct `{name}` — first definition at byte {}, \
        duplicate at byte {} \
        — rename one of the conflicting definitions",
        first_span.start, duplicate_span.start)]
    DuplicateStruct {
        name: String,
        first_span: Span,
        duplicate_span: Span,
    },

    /// Struct name used in a struct literal is not defined.
    #[error("unknown struct `{name}` at byte {} — no struct with this name is defined\n\
        suggestion: check the struct name or add a `struct {name}` declaration",
        span.start)]
    UndefinedStruct { name: String, span: Span },

    /// A field name appears more than once in a struct literal.
    #[error("struct `{struct_name}` initialized with duplicate field `{field_name}` at byte {} \
        (first use at byte {})\n\
        suggestion: remove the duplicate `{field_name}` initializer",
        duplicate_span.start, first_span.start)]
    DuplicateStructField {
        struct_name: String,
        field_name: String,
        first_span: Span,
        duplicate_span: Span,
    },

    /// One or more fields of a struct are not initialized in a struct literal.
    #[error("struct `{struct_name}` is missing fields at byte {}: {}\n\
        suggestion: add initializers for the missing fields",
        span.start, missing.join(", "))]
    MissingStructFields { struct_name: String, missing: Vec<String>, span: Span },

    /// Field name not found in the struct's field table (§23).
    /// Emitted for `obj.field` when the struct has no field named `field`.
    #[error("field `{field_name}` not found on type `{type_name}` at byte {}{}\n\
        suggestion: check the field name against the definition of `{type_name}`",
        span.start,
        if available.is_empty() { " — type has no fields".to_string() }
        else { format!(" — available fields: {}", available.join(", ")) })]
    FieldNotFound {
        type_name: String,
        field_name: String,
        span: Span,
        available: Vec<String>,
    },

    /// Writing to a field through a shared (`&T`) reference — requires ownership or `&mut T`.
    /// Detected at the type level without borrow-checker machinery (§23).
    #[error("cannot assign to `{type_name}::{field_name}` through a shared reference at byte {}\n\
        note: the receiver is a shared borrow (`&{type_name}`) — field mutation requires \
        either a mutable borrow (`&mut {type_name}`) or an owned value\n\
        suggestion: use a `&mut {type_name}` receiver, or restructure so the owning \
        binding is used directly",
        span.start)]
    FieldWriteViaSharedRef {
        type_name: String,
        field_name: String,
        span: Span,
    },

    /// Moving a non-Copy field out of a struct — partial moves not yet supported (§23).
    /// Detected in let bindings, return statements, and function-call arguments where
    /// consuming intent is unambiguous from call-site context.
    #[error("cannot move `{type_name}::{field_name}` out of a field access at byte {}\n\
        note: moving a single field out of a struct requires tracking that the struct \
        is partially moved, which is not implemented in this compiler phase\n\
        suggestion: either move the whole struct, or access `{field_name}` only by borrow",
        span.start)]
    FieldOwnNonCopy {
        type_name: String,
        field_name: String,
        span: Span,
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
    #[allow(dead_code)]
    #[error("lifetime conflict at byte {}: {note}{}", span.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    LifetimeConflict {
        span: Span,
        note: String,
        suggestion: Option<String>,
    },

    /// Use of a moved value. Phase 2.
    #[allow(dead_code)]
    #[error("use of moved value `{name}` at byte {}; value was moved at byte {}{}", use_span.start, moved_at.start,
        suggestion.as_deref().map(|s| format!(" — {s}")).unwrap_or_default())]
    UseAfterMove {
        name: String,
        use_span: Span,
        moved_at: Span,
        suggestion: Option<String>,
    },

    /// Conflicting borrows. Phase 2.
    #[allow(dead_code)]
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
