use std::collections::HashMap;
use crate::lexer::token::Span;
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{FnSig, Ty};

/// Lexical scope stack. Each scope maps a variable name to its inferred type.
/// `push_scope` / `pop_scope` bracket every block; `lookup` walks inward → outward.
pub(crate) struct Env {
    scopes: Vec<HashMap<String, Ty>>,
}

impl Env {
    pub(crate) fn new() -> Self {
        Env { scopes: vec![HashMap::new()] }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn define(&mut self, name: &str, ty: Ty) {
        self.scopes.last_mut().expect("scope stack empty").insert(name.to_string(), ty);
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&Ty> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }
}

/// Global inference context threaded through the entire checking pass.
pub(crate) struct InferCtx {
    /// Top-level function signatures + definition span, populated by the collection
    /// pass before any body is checked. Enables mutual recursion. Span is used to
    /// cite both sites when a duplicate name is detected (pillar 1 + pillar 5).
    pub(crate) fn_sigs: HashMap<String, (FnSig, Span)>,

    /// Per-type method namespaces, keyed by type name then method/associated-fn name.
    /// Populated during the collection pass alongside fn_sigs. Used now for
    /// whole-program duplicate detection (§22); method dispatch in a future session.
    pub(crate) impl_sigs: HashMap<String, HashMap<String, (FnSig, Span)>>,

    /// Span → inferred type. Codegen queries this to determine LLVM operand types.
    /// Keyed by the expression span from the AST.
    pub(crate) type_map: HashMap<Span, Ty>,

    /// Errors accumulated during inference. Non-fatal errors (e.g. `Deferred`)
    /// are collected here but do not prevent inference from continuing.
    pub(crate) errors: Vec<TypeError>,

    // ── Phase 2 hooks (not yet implemented) ───────────────────────────────────
    // Uncomment when Hindley-Milner unification is introduced:
    // ty_var_count: u32,
    // ty_var_subst: Vec<Option<Ty>>,
    //
    // Uncomment when region/lifetime inference is introduced:
    // region_var_count: u32,
    // region_constraints: Vec<RegionConstraint>,
}

impl InferCtx {
    pub(crate) fn new() -> Self {
        InferCtx {
            fn_sigs: HashMap::new(),
            impl_sigs: HashMap::new(),
            type_map: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Record the inferred type for a given source span. Called after every
    /// `infer_expr` and `infer_block` so codegen can look up types by span.
    ///
    /// Idempotent for the same (span, type) pair — `Expr::Block(b)` legitimately
    /// records `b.span` from both `infer_block` and `infer_expr`. Panics in debug
    /// builds if the same span is recorded with *different* types, which indicates
    /// a genuine collision between two semantically distinct nodes.
    pub(crate) fn record(&mut self, span: Span, ty: Ty) {
        if let Some(prev) = self.type_map.get(&span) {
            debug_assert!(
                prev == &ty,
                "span collision with conflicting types at byte {}: had {prev:?}, inserting {ty:?}",
                span.start
            );
            return;
        }
        self.type_map.insert(span, ty);
    }

    /// Push a non-fatal or fatal type error. Inference continues in both cases;
    /// the caller checks `InferCtx::has_fatal_errors` at the end.
    pub(crate) fn error(&mut self, e: TypeError) {
        self.errors.push(e);
    }

    pub(crate) fn has_fatal_errors(&self) -> bool {
        self.errors.iter().any(TypeError::is_fatal)
    }

    // PHASE2: pub(crate) fn fresh_tyvar(&mut self) -> Ty { ... }
    // PHASE2: pub(crate) fn fresh_region_var(&mut self) -> Region { ... }
    // PHASE2: pub(crate) fn unify(&mut self, a: &Ty, b: &Ty, span: Span) { ... }
}
