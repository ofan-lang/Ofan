use std::collections::HashMap;
use crate::ast::CopyMove;
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

/// Resolved enum definition stored in InferCtx after collection sub-passes.
pub(crate) struct EnumInfo {
    pub(crate) name_span: Span,
    /// Resolved variant payload types, keyed by variant name.
    /// Empty Vec = unit variant. Non-empty = tuple variant (fields in order).
    pub(crate) variants: HashMap<String, Vec<crate::typechecker::ty::Ty>>,
    /// Variant names in source order — used for `available` lists in VariantNotFound.
    pub(crate) variant_order: Vec<String>,
    pub(crate) copy_override: Option<CopyMove>,
    pub(crate) is_generic: bool,
}

/// Resolved struct definition stored in InferCtx after collection sub-passes.
pub(crate) struct StructInfo {
    /// Span of the struct name — used to cite the first definition in DuplicateStruct.
    pub(crate) name_span: Span,
    /// Resolved field types, keyed by field name.
    pub(crate) fields: HashMap<String, Ty>,
    /// Field names in source order — used for `available` lists in FieldNotFound errors.
    pub(crate) field_order: Vec<String>,
    /// Explicit Copy/Move override from the struct modifier keyword (§23).
    ///   Some(CopyMove::Copy) = `copy struct` → always Copy
    ///   Some(CopyMove::Move) = `move struct` → never Copy
    ///   None                 = infer from field types
    pub(crate) copy_override: Option<CopyMove>,
    /// True when the struct has generic parameters — field access deferred in phase 1.
    pub(crate) is_generic: bool,
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

    /// Struct definitions collected in sub-passes 1a/1c, keyed by struct name.
    /// Queried by infer_field_access and is_copy (§23).
    pub(crate) struct_defs: HashMap<String, StructInfo>,

    /// Enum definitions collected in sub-passes 1b/1d, keyed by enum name.
    /// Queried by infer_field_access (qualified variant) and is_copy (§20).
    pub(crate) enum_defs: HashMap<String, EnumInfo>,

    /// Maps variant name → list of enum names that declare it.
    /// Vec len > 1 means the bare form is ambiguous at use sites (§20).
    pub(crate) variant_to_enum: HashMap<String, Vec<String>>,

    /// Span → inferred type. Codegen queries this to determine LLVM operand types.
    /// Keyed by the expression span from the AST.
    pub(crate) type_map: HashMap<Span, Ty>,

    /// Errors accumulated during inference. Non-fatal errors (e.g. `Deferred`)
    /// are collected here but do not prevent inference from continuing.
    pub(crate) errors: Vec<TypeError>,

    /// Declared return type of the function currently being inferred.
    /// Stored as a stack so nested `fn` items (not in Ofan today, but anticipated)
    /// do not clobber the outer function's return type. A flat field would silently
    /// misbehave the moment nested functions are added. Pushed on entry to
    /// `infer_fn`/`infer_method`, popped on exit — every push/pop is paired.
    pub(crate) current_return_ty: Vec<Ty>,

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
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            variant_to_enum: HashMap::new(),
            type_map: HashMap::new(),
            errors: Vec::new(),
            current_return_ty: Vec::new(),
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
