/// Internal type representation used throughout the typechecker.
///
/// Ty is not the same as `ast::Type` — it is the *resolved* form after names
/// have been looked up and primitive identifiers have been collapsed to variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    // ── Primitives ────────────────────────────────────────────────────────────
    I32,
    F64,
    Bool,
    Char,
    /// The `str` primitive. Always used behind a reference in practice, but
    /// represented bare here so `Ty::Ref { inner: Box::new(Ty::Str) }` reads cleanly.
    Str,
    /// Unit type `()` — the type of void functions and discarded expression statements.
    Unit,

    // ── Compound types ────────────────────────────────────────────────────────

    /// Reference type `&[mut] [region] T`.
    ///
    /// `region` is `None` in phase 1 — lifetime tracking is deferred.
    /// Phase 2: populate `region` and enforce borrow/lifetime rules.
    /// The field is present now so pattern matches on `Ref` don't need updating
    /// when phase 2 introduces region vars.
    Ref { mutable: bool, region: Option<Region>, inner: Box<Ty> },

    /// Named/user-defined type (struct, enum) or an unrecognised name.
    /// Used when `ast::Type::Named` doesn't map to a primitive or generic param.
    Named(String),

    /// Unsubstituted generic type parameter (e.g. `T`, `E`).
    /// Phase 2: replace via unification when generic calls are instantiated.
    Param(String),

    /// Unification variable — allocated from `InferCtx::fresh_tyvar()`.
    ///
    /// Included now so `TypeError` variants can mention it without
    /// an API break when unification is introduced.
    /// **Never constructed in phase 1** — any code path that constructs it is a
    /// compiler bug and should be caught by the exhaustive match in `infer_expr`.
    #[allow(clippy::enum_variant_names)]
    #[allow(dead_code)] // phase 2: used when unification is introduced
    TyVar(u32),

    /// Error sentinel used for deferred or failed sub-expressions.
    ///
    /// Any `TypeError::Mismatch` where either side is `Ty::Error` is silently
    /// suppressed to avoid cascading diagnostics from a single root error.
    Error,
}

/// Region (lifetime) for reference types.
///
/// All variants are present now to avoid breaking `Ty::Ref` when region vars
/// land in phase 2.
#[derive(Debug, Clone, PartialEq)]
pub enum Region {
    /// Named region tag, e.g. `r1`, `r2` (bare identifiers in `<>` param lists).
    Named(String),
    /// `&static` — lives for the whole program.
    Static,
    // PHASE2: Var(u32) — region inference variable, allocated from
    // InferCtx::fresh_region_var(). Never constructed in phase 1.
}

/// Resolved signature of a function or method, used for call-site checking.
#[derive(Debug, Clone)]
pub struct FnSig {
    /// Parameter types. For methods, the self receiver has already been stripped;
    /// call sites must NOT pass self as an explicit argument.
    pub params: Vec<Ty>,
    pub return_ty: Ty,
    /// True when the function has generic params (`fn f<T>(...)`).
    /// Phase 1: call-site type checking is deferred for generic functions.
    pub is_generic: bool,
    /// True when this method's self receiver is `move self` (consumes ownership).
    /// Always `false` for free functions. Used in `infer_method_call` to reject
    /// calling a consuming method through a shared/mutable reference.
    pub self_consuming: bool,
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::I32 => write!(f, "i32"),
            Ty::F64 => write!(f, "f64"),
            Ty::Bool => write!(f, "bool"),
            Ty::Char => write!(f, "char"),
            Ty::Str => write!(f, "str"),
            Ty::Unit => write!(f, "unit"),
            Ty::Named(n) | Ty::Param(n) => write!(f, "{n}"),
            Ty::Ref { mutable: true, inner, .. } => write!(f, "&mut {inner}"),
            Ty::Ref { mutable: false, inner, .. } => write!(f, "&{inner}"),
            Ty::TyVar(id) => write!(f, "?{id}"),
            Ty::Error => write!(f, "<error>"),
        }
    }
}
