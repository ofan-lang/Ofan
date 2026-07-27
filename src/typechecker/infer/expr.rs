use std::collections::{HashMap, HashSet};
use crate::ast::{Expr, Literal, MatchArm, Pattern, StructFieldInit};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{Region, Ty};

// ─── Expression inference ─────────────────────────────────────────────────────

pub(super) fn infer_expr(expr: &Expr<'_>, ctx: &mut InferCtx, env: &mut Env) -> Ty {
    let ty = infer_expr_inner(expr, ctx, env);
    ctx.record(expr.span(), ty.clone());
    ty
}

fn infer_expr_inner(expr: &Expr<'_>, ctx: &mut InferCtx, env: &mut Env) -> Ty {
    match expr {
        // ── Literals ─────────────────────────────────────────────────────────
        Expr::Literal(lit, span) => {
            if let Literal::Integer(n) = lit {
                if *n > i32::MAX as i64 {
                    ctx.error(TypeError::IntegerOutOfRange { value: *n, span: *span });
                    return Ty::Error;
                }
            }
            infer_literal(lit)
        }

        // ── Identifier ───────────────────────────────────────────────────────
        Expr::Ident(name, span) => {
            if let Some(ty) = env.lookup(name) {
                return ty.clone();
            }
            if let Some((sig, _)) = ctx.fn_sigs.get(*name) {
                return sig.return_ty.clone();
            }
            // Bare variant name lookup (§20). Clone the enum name list to avoid
            // holding a borrow into ctx while we call ctx.error below.
            if let Some(enum_names) = ctx.variant_to_enum.get(*name).cloned() {
                if enum_names.len() > 1 {
                    ctx.error(TypeError::AmbiguousVariant {
                        variant_name: name.to_string(),
                        defined_in: enum_names,
                        span: *span,
                    });
                    return Ty::Error;
                }
                let enum_name = enum_names.into_iter().next().unwrap();
                // Check is_generic and variant kind by reading the enum info once.
                let (is_generic, variant_is_unit) = {
                    let info = ctx.enum_defs.get(&enum_name);
                    let gen = info.map(|i| i.is_generic).unwrap_or(false);
                    let unit = info.and_then(|i| i.variants.get(*name)).map(|f| f.is_empty());
                    (gen, unit)
                };
                if is_generic {
                    return super::defer(
                        ctx,
                        "bare variant on generic enum — requires type instantiation",
                        *span,
                    );
                }
                match variant_is_unit {
                    Some(true) => return Ty::Named(enum_name),
                    Some(false) => {
                        ctx.error(TypeError::TupleVariantUsedAsUnit {
                            enum_name,
                            variant_name: name.to_string(),
                            span: *span,
                        });
                        return Ty::Error;
                    }
                    None => {} // variant_to_enum invariant violated; fall through
                }
            }
            ctx.error(TypeError::UndefinedVariable {
                name: name.to_string(),
                span: *span,
                suggestion: Some("check spelling, or declare the variable with `let`".to_string()),
            });
            Ty::Error
        }

        // ── Unary ────────────────────────────────────────────────────────────
        Expr::Unary { op, expr, span } => super::ops::infer_unary(*op, expr, *span, ctx, env),

        // ── Binary ───────────────────────────────────────────────────────────
        Expr::Binary { op, left, right, span } => {
            super::ops::infer_binary(*op, left, right, *span, ctx, env)
        }

        // ── Function call ─────────────────────────────────────────────────────
        Expr::Call { callee, args, span } => infer_call(callee, args, *span, ctx, env),

        // ── Block expression ──────────────────────────────────────────────────
        Expr::Block(block) => {
            // Thread the enclosing function's declared return type so that `return`
            // statements inside block expressions type-check against the right type.
            // The return type is stored on ctx.current_return_ty (a stack — see env.rs).
            let ret = ctx.current_return_ty.last().cloned().unwrap_or(Ty::Unit);
            super::infer_block(block, &ret, ctx, env)
        }

        // ── If expression ─────────────────────────────────────────────────────
        Expr::If { condition, then_block, else_branch, span } => {
            let cond_ty = infer_expr(condition, ctx, env);
            if !matches!(cond_ty, Ty::Bool | Ty::Error) {
                ctx.error(TypeError::NonBoolCondition { found: cond_ty, span: *span });
            }
            let ret = ctx.current_return_ty.last().cloned().unwrap_or(Ty::Unit);
            let then_ty = super::infer_block(then_block, &ret, ctx, env);
            match else_branch {
                Some(else_expr) => {
                    let else_ty = infer_expr(else_expr, ctx, env);
                    if !matches!((&then_ty, &else_ty), (Ty::Error, _) | (_, Ty::Error))
                        && then_ty != else_ty
                    {
                        ctx.error(TypeError::BranchMismatch {
                            then: then_ty,
                            else_: else_ty,
                            span: *span,
                        });
                        return Ty::Error;
                    }
                    then_ty
                }
                None => Ty::Unit,
            }
        }

        // ── While loop ────────────────────────────────────────────────────────
        Expr::While { condition, body, span } => {
            let cond_ty = infer_expr(condition, ctx, env);
            if !matches!(cond_ty, Ty::Bool | Ty::Error) {
                ctx.error(TypeError::NonBoolCondition { found: cond_ty, span: *span });
            }
            let ret = ctx.current_return_ty.last().cloned().unwrap_or(Ty::Unit);
            super::infer_block(body, &ret, ctx, env);
            Ty::Unit
        }

        // ── Loop expression ───────────────────────────────────────────────────
        Expr::Loop { body, span: _ } => {
            // Phase 1: treat as unit. Break-value type inference is deferred.
            let ret = ctx.current_return_ty.last().cloned().unwrap_or(Ty::Unit);
            super::infer_block(body, &ret, ctx, env);
            Ty::Unit
        }

        // ── Deferred expressions ──────────────────────────────────────────────

        Expr::MethodCall { object, method, method_span, args, span } => {
            infer_method_call(object, method, *method_span, args, *span, ctx, env)
        }

        Expr::Field { object, field, field_span, span } => {
            infer_field_access(object, field, *field_span, *span, ctx, env)
        }

        // PHASE2: cast rules not yet spec'd
        Expr::Cast { span, .. } => super::defer(ctx, "cast (as) — rules not yet spec'd", *span),

        // PHASE2: requires Checked<T, E> impl
        Expr::Propagate { span, .. } => {
            super::defer(ctx, "? operator — requires Checked<T, E> design", *span)
        }

        // PHASE2: requires iterator trait
        Expr::For { span, .. } => super::defer(ctx, "for loops — requires iterator trait", *span),

        Expr::Match { subject, arms, span } => infer_match(subject, arms, *span, ctx, env),

        Expr::StructLit { name, name_span, fields, span } => {
            infer_struct_lit(name, *name_span, fields, *span, ctx, env)
        }
    }
}

// ─── Literal typing ───────────────────────────────────────────────────────────

fn infer_literal(lit: &Literal<'_>) -> Ty {
    match lit {
        Literal::Integer(_) => Ty::I32,
        Literal::Float(_) => Ty::F64,
        Literal::Bool(_) => Ty::Bool,
        Literal::Char(_) => Ty::Char,
        Literal::Str(_) => Ty::Ref {
            mutable: false,
            region: Some(Region::Static),
            inner: Box::new(Ty::Str),
        },
    }
}

// ─── Method call typing ───────────────────────────────────────────────────────

fn infer_method_call(
    object: &Expr<'_>,
    method: &str,
    method_span: Span,
    args: &[Expr<'_>],
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Option B hook: qualified tuple variant construction (`EnumName.Variant(args)`).
    // `Shape.Circle(3.14)` parses as MethodCall { object: Ident("Shape"), method: "Circle" }.
    // Must fire BEFORE infer_expr(object) — same reason as the Field hook above.
    if let Expr::Ident(type_name, _) = object {
        if env.lookup(type_name).is_none() {
            if let Some(info) = ctx.enum_defs.get(*type_name) {
                if info.is_generic {
                    infer_all(args, ctx, env);
                    return super::defer(
                        ctx,
                        "qualified variant on generic enum — requires type instantiation",
                        span,
                    );
                }
                let variant_order = info.variant_order.clone();
                let variant_fields = info.variants.get(method).cloned();
                let enum_name = type_name.to_string();
                match variant_fields {
                    None => {
                        infer_all(args, ctx, env);
                        ctx.error(TypeError::VariantNotFound {
                            enum_name,
                            variant_name: method.to_string(),
                            span: method_span,
                            available: variant_order,
                        });
                        return Ty::Error;
                    }
                    Some(fields) if fields.is_empty() => {
                        infer_all(args, ctx, env);
                        ctx.error(TypeError::UnitVariantCalledAsFunction {
                            enum_name,
                            variant_name: method.to_string(),
                            span,
                        });
                        return Ty::Error;
                    }
                    Some(field_tys) => {
                        if args.len() != field_tys.len() {
                            infer_all(args, ctx, env);
                            ctx.error(TypeError::VariantArgCountMismatch {
                                enum_name: enum_name.clone(),
                                variant_name: method.to_string(),
                                expected: field_tys.len(),
                                found: args.len(),
                                span,
                                suggestion: Some(format!(
                                    "pass exactly {} argument(s) to `{enum_name}::{method}`",
                                    field_tys.len()
                                )),
                            });
                            return Ty::Error;
                        }
                        for (arg, expected_ty) in args.iter().zip(field_tys.iter()) {
                            let arg_ty = infer_expr(arg, ctx, env);
                            super::check_types(expected_ty, &arg_ty, arg.span(), ctx, || {
                                Some(format!(
                                    "variant `{enum_name}::{method}` field expects `{expected_ty:?}`"
                                ))
                            });
                        }
                        return Ty::Named(enum_name);
                    }
                }
            }
        }
    }

    let recv_ty = infer_expr(object, ctx, env);

    // Cascade suppression: don't pile errors onto an already-errored receiver.
    if matches!(recv_ty, Ty::Error) {
        infer_all(args, ctx, env);
        return Ty::Error;
    }

    // Auto-deref one Ref layer to find the dispatchable type name.
    let type_name = match dispatch_type_name(&recv_ty) {
        Some(n) => n.to_string(),
        None => {
            infer_all(args, ctx, env);
            ctx.error(TypeError::MethodNotFound {
                type_name: format!("{recv_ty}"),
                method_name: method.to_string(),
                span: method_span,
                suggestion: Some(format!("type `{recv_ty}` has no impl block")),
            });
            return Ty::Error;
        }
    };

    let sig = match ctx.impl_sigs.get(&type_name).and_then(|ns| ns.get(method)) {
        Some((s, _)) => s.clone(),
        None => {
            infer_all(args, ctx, env);
            let available: Vec<String> = ctx.impl_sigs
                .get(&type_name)
                .map(|ns| {
                    let mut names: Vec<String> = ns.keys().cloned().collect();
                    names.sort();
                    names
                })
                .unwrap_or_default();
            let suggestion = if available.is_empty() {
                format!("type `{type_name}` has no impl block")
            } else {
                format!("type `{type_name}` has methods: {}", available.join(", "))
            };
            ctx.error(TypeError::MethodNotFound {
                type_name: type_name.clone(),
                method_name: method.to_string(),
                span: method_span,
                suggestion: Some(suggestion),
            });
            return Ty::Error;
        }
    };

    // Reject calling a move-self method through a reference — this is a type-level
    // violation (cannot move out of a borrowed value), detectable without lifetime machinery.
    if sig.self_consuming {
        if let Ty::Ref { .. } = &recv_ty {
            infer_all(args, ctx, env);
            ctx.error(TypeError::ConsumeViaRef {
                type_name: type_name.clone(),
                method_name: method.to_string(),
                span,
            });
            return Ty::Error;
        }
    }

    if sig.is_generic {
        infer_all(args, ctx, env);
        return super::defer(ctx, "generic method call instantiation — unification not yet implemented", span);
    }

    // sig.params does NOT include self (stripped in collect_impl_sigs).
    if args.len() != sig.params.len() {
        infer_all(args, ctx, env);
        ctx.error(TypeError::ArgCountMismatch {
            name: format!("{type_name}::{method}"),
            expected: sig.params.len(),
            found: args.len(),
            span,
            suggestion: Some(format!(
                "pass exactly {} argument(s) to `{type_name}::{method}`",
                sig.params.len()
            )),
        });
        return Ty::Error;
    }

    let mut any_error = false;
    for (i, (arg, expected_ty)) in args.iter().zip(&sig.params).enumerate() {
        let arg_ty = infer_expr(arg, ctx, env);
        // Skip FieldOwnNonCopy when expected type is a ref — type-mismatch fires instead.
        if !matches!(expected_ty, Ty::Ref { .. })
            && super::check_tail_field_own_non_copy(arg, ctx)
        {
            any_error = true;
            continue;
        }
        if !matches!(arg_ty, Ty::Error) && &arg_ty != expected_ty {
            ctx.error(TypeError::Mismatch {
                expected: expected_ty.clone(),
                found: arg_ty,
                span: arg.span(),
                suggestion: Some(format!(
                    "argument {} of `{type_name}::{method}` expects `{expected_ty:?}`",
                    i + 1
                )),
            });
            any_error = true;
        }
    }

    if any_error { Ty::Error } else { sig.return_ty.clone() }
}

fn infer_all(args: &[Expr<'_>], ctx: &mut InferCtx, env: &mut Env) {
    for arg in args { infer_expr(arg, ctx, env); }
}

// ─── Field access typing ──────────────────────────────────────────────────────

fn infer_field_access(
    object: &Expr<'_>,
    field: &str,
    field_span: Span,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Option B hook: qualified unit variant construction (`EnumName.VariantName`).
    // Must fire BEFORE infer_expr(object) because enum names are type identifiers,
    // not value bindings — the normal infer path would emit UndefinedVariable on them.
    // Local bindings shadow enum names (env.lookup check first, per spec §20).
    if let Expr::Ident(type_name, type_name_span) = object {
        if env.lookup(type_name).is_none() {
            if let Some(info) = ctx.enum_defs.get(*type_name) {
                if info.is_generic {
                    return super::defer(
                        ctx,
                        "qualified variant on generic enum — requires type instantiation",
                        span,
                    );
                }
                let variant_order = info.variant_order.clone();
                let variant_fields = info.variants.get(field).cloned();
                let enum_name = type_name.to_string();
                // Record the enum name identifier's type so that
                // check_tail_field_own_non_copy can correctly identify this as an enum
                // type (not a struct field access) and not emit FieldOwnNonCopy.
                ctx.record(*type_name_span, Ty::Named(enum_name.clone()));
                match variant_fields {
                    None => {
                        ctx.error(TypeError::VariantNotFound {
                            enum_name,
                            variant_name: field.to_string(),
                            span: field_span,
                            available: variant_order,
                        });
                        return Ty::Error;
                    }
                    Some(fields) if !fields.is_empty() => {
                        // Tuple variant used without args — should be EnumName.Variant(...)
                        ctx.error(TypeError::TupleVariantUsedAsUnit {
                            enum_name,
                            variant_name: field.to_string(),
                            span: field_span,
                        });
                        return Ty::Error;
                    }
                    Some(_) => return Ty::Named(enum_name),
                }
            }
        }
    }

    let obj_ty = infer_expr(object, ctx, env);
    if matches!(obj_ty, Ty::Error) { return Ty::Error; }

    // Auto-deref one Ref layer to find the underlying struct type.
    let effective_ty = match &obj_ty {
        Ty::Ref { inner, .. } => inner.as_ref().clone(),
        t => t.clone(),
    };

    let type_name = match &effective_ty {
        Ty::Named(n) => n.clone(),
        _ => return super::defer(ctx, "field access on non-struct type", span),
    };

    let info = match ctx.struct_defs.get(&type_name) {
        Some(i) => i,
        None => return super::defer(ctx, "field access — struct type not in table", span),
    };

    if info.is_generic {
        return super::defer(ctx, "field access on generic struct — requires type instantiation", span);
    }

    match info.fields.get(field) {
        Some(ty) => ty.clone(),
        None => {
            let available = info.field_order.clone();
            ctx.error(TypeError::FieldNotFound {
                type_name, field_name: field.to_string(), span: field_span, available,
            });
            Ty::Error
        }
    }
}

/// Extract the type name to use for method dispatch.
/// Auto-derefs through exactly one level of `&` / `&mut` reference.
fn dispatch_type_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Named(n) => Some(n.as_str()),
        Ty::Ref { inner, .. } => {
            if let Ty::Named(n) = inner.as_ref() { Some(n.as_str()) } else { None }
        }
        _ => None,
    }
}

// ─── Function call typing ─────────────────────────────────────────────────────

fn infer_call(
    callee: &Expr<'_>,
    args: &[Expr<'_>],
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Phase 1: only handle direct function calls where callee is an identifier.
    let name = match callee {
        Expr::Ident(n, _) => *n,
        _ => {
            // Infer callee anyway (for its side effects on the type map).
            infer_expr(callee, ctx, env);
            for arg in args {
                infer_expr(arg, ctx, env);
            }
            return super::defer(ctx, "non-identifier callees (function pointers, closures)", span);
        }
    };

    // Look up in environment first (local binding shadows function name).
    if env.lookup(name).is_some() {
        // A local variable is being called as a function — not supported in phase 1.
        for arg in args {
            infer_expr(arg, ctx, env);
        }
        return super::defer(ctx, "calling local variables as functions (closures/fn pointers)", span);
    }

    // Bare tuple variant constructor: `Circle(3.14)` where Circle ∈ variant_to_enum (§20).
    if let Some(enum_names) = ctx.variant_to_enum.get(name).cloned() {
        if enum_names.len() > 1 {
            infer_all(args, ctx, env);
            ctx.error(TypeError::AmbiguousVariant {
                variant_name: name.to_string(),
                defined_in: enum_names,
                span,
            });
            return Ty::Error;
        }
        let enum_name = enum_names.into_iter().next().unwrap();
        let (enum_is_generic, field_tys) = {
            let info = ctx.enum_defs.get(&enum_name);
            let gen = info.map(|i| i.is_generic).unwrap_or(false);
            let tys = info.and_then(|i| i.variants.get(name)).cloned().unwrap_or_default();
            (gen, tys)
        };
        if enum_is_generic {
            infer_all(args, ctx, env);
            return super::defer(ctx, "bare variant on generic enum — requires type instantiation", span);
        }
        if field_tys.is_empty() {
            infer_all(args, ctx, env);
            ctx.error(TypeError::UnitVariantCalledAsFunction {
                enum_name,
                variant_name: name.to_string(),
                span,
            });
            return Ty::Error;
        }
        if args.len() != field_tys.len() {
            infer_all(args, ctx, env);
            ctx.error(TypeError::VariantArgCountMismatch {
                enum_name: enum_name.clone(),
                variant_name: name.to_string(),
                expected: field_tys.len(),
                found: args.len(),
                span,
                suggestion: Some(format!(
                    "pass exactly {} argument(s) to `{enum_name}::{name}`",
                    field_tys.len()
                )),
            });
            return Ty::Error;
        }
        for (arg, expected_ty) in args.iter().zip(field_tys.iter()) {
            let arg_ty = infer_expr(arg, ctx, env);
            super::check_types(expected_ty, &arg_ty, arg.span(), ctx, || {
                Some(format!(
                    "field of `{enum_name}::{name}` expects `{expected_ty:?}`"
                ))
            });
        }
        return Ty::Named(enum_name);
    }

    let sig = match ctx.fn_sigs.get(name) {
        Some((s, _)) => s.clone(),
        None => {
            for arg in args {
                infer_expr(arg, ctx, env);
            }
            ctx.error(TypeError::UndefinedFunction {
                name: name.to_string(),
                span,
                suggestion: Some("check spelling, or add a `fn` declaration".to_string()),
            });
            return Ty::Error;
        }
    };

    // Generic functions: skip detailed checking in phase 1.
    if sig.is_generic {
        for arg in args {
            infer_expr(arg, ctx, env);
        }
        return super::defer(
            ctx,
            "generic function call instantiation — unification not yet implemented",
            span,
        );
    }

    // Arg count check.
    if args.len() != sig.params.len() {
        for arg in args {
            infer_expr(arg, ctx, env);
        }
        ctx.error(TypeError::ArgCountMismatch {
            name: name.to_string(),
            expected: sig.params.len(),
            found: args.len(),
            span,
            suggestion: Some(format!(
                "pass exactly {} argument(s), or change the signature of `{name}`",
                sig.params.len()
            )),
        });
        return Ty::Error;
    }

    // Arg type checks.
    let mut any_error = false;
    for (i, (arg, expected_ty)) in args.iter().zip(&sig.params).enumerate() {
        let arg_ty = infer_expr(arg, ctx, env);
        // Skip FieldOwnNonCopy when expected type is a ref — type-mismatch fires instead.
        if !matches!(expected_ty, Ty::Ref { .. })
            && super::check_tail_field_own_non_copy(arg, ctx)
        {
            any_error = true;
            continue;
        }
        if !matches!(arg_ty, Ty::Error) && &arg_ty != expected_ty {
            ctx.error(TypeError::Mismatch {
                expected: expected_ty.clone(),
                found: arg_ty,
                span: arg.span(),
                suggestion: Some(format!(
                    "argument {} of `{name}` expects `{expected_ty:?}`",
                    i + 1,
                )),
            });
            any_error = true;
        }
    }

    if any_error { Ty::Error } else { sig.return_ty.clone() }
}

// ─── Struct literal inference ─────────────────────────────────────────────────

fn infer_struct_lit(
    name: &str,
    name_span: Span,
    fields: &[StructFieldInit<'_>],
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Extract what we need from the borrow before any &mut ctx calls.
    let (field_order, field_types, is_generic) = match ctx.struct_defs.get(name) {
        None => {
            ctx.error(TypeError::UndefinedStruct { name: name.to_string(), span: name_span });
            return Ty::Error;
        }
        Some(info) => {
            if info.is_generic {
                return super::defer(
                    ctx,
                    "struct literal of generic struct — requires type instantiation",
                    span,
                );
            }
            (info.field_order.clone(), info.fields.clone(), info.is_generic)
        }
    };
    let _ = is_generic; // extracted for symmetry; used as early-return guard above

    let available = field_order.clone();
    let mut seen: HashMap<&str, Span> = HashMap::new();

    for field_init in fields {
        if let Some(&first_span) = seen.get(field_init.name) {
            ctx.error(TypeError::DuplicateStructField {
                struct_name: name.to_string(),
                field_name: field_init.name.to_string(),
                first_span,
                duplicate_span: field_init.name_span,
            });
            infer_expr(&field_init.value, ctx, env);
            continue;
        }
        seen.insert(field_init.name, field_init.name_span);

        let val_ty = infer_expr(&field_init.value, ctx, env);

        match field_types.get(field_init.name) {
            None => ctx.error(TypeError::FieldNotFound {
                type_name: name.to_string(),
                field_name: field_init.name.to_string(),
                span: field_init.name_span,
                available: available.clone(),
            }),
            Some(expected_ty) => {
                if val_ty != Ty::Error && *expected_ty != val_ty {
                    ctx.error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: val_ty,
                        span: field_init.value.span(),
                        suggestion: Some(format!(
                            "field `{}` of `{}` expects `{:?}`",
                            field_init.name, name, expected_ty
                        )),
                    });
                }
            }
        }
    }

    let missing: Vec<String> = field_order.iter()
        .filter(|f| !seen.contains_key(f.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        ctx.error(TypeError::MissingStructFields {
            struct_name: name.to_string(),
            missing,
            span,
        });
    }

    Ty::Named(name.to_string())
}

// ─── Match expression inference ───────────────────────────────────────────────

fn infer_match(
    subject: &Expr<'_>,
    arms: &[MatchArm<'_>],
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    let subject_ty = infer_expr(subject, ctx, env);

    let mut first_arm_ty: Option<Ty> = None;
    // `catchall_span` doubles as the has-catchall flag — `Some` means a catch-all was seen.
    let mut catchall_span: Option<Span> = None;

    // Coverage tracking for exhaustiveness.
    let mut covered: HashSet<String> = HashSet::new();
    let mut true_covered = false;
    let mut false_covered = false;

    for arm in arms {
        // Unreachable arm detection — flag but keep inferring for error recovery.
        if let Some(cs) = catchall_span {
            ctx.error(TypeError::UnreachableArm { span: arm.span, catch_all_span: cs });
        }

        env.push_scope();

        // Guarded arms do NOT count toward exhaustiveness coverage. Route their pattern
        // into a temporary set so real coverage is only updated by unguarded arms.
        let is_guarded = arm.guard.is_some();
        let mut temp_covered = HashSet::new();
        let mut temp_tc = false;
        let mut temp_fc = false;
        let (cov, tc, fc) = if is_guarded {
            (&mut temp_covered, &mut temp_tc, &mut temp_fc)
        } else {
            (&mut covered, &mut true_covered, &mut false_covered)
        };

        let mut arm_is_catchall = check_pattern(
            &arm.pattern,
            &subject_ty,
            ctx,
            env,
            cov,
            tc,
            fc,
        );

        // Guard: must produce bool; a guarded arm never counts as a catch-all.
        if let Some(guard) = &arm.guard {
            let guard_ty = infer_expr(guard, ctx, env);
            if !matches!(guard_ty, Ty::Bool | Ty::Error) {
                ctx.error(TypeError::NonBoolCondition { found: guard_ty, span: guard.span() });
            }
            arm_is_catchall = false;
        }

        let arm_body_ty = infer_expr(&arm.body, ctx, env);
        ctx.record(arm.span, arm_body_ty.clone());

        env.pop_scope();

        // Arm type unification — all arms must agree on one type.
        match &first_arm_ty {
            None => first_arm_ty = Some(arm_body_ty.clone()),
            Some(ref_ty)
                if ref_ty != &arm_body_ty
                    && !matches!((ref_ty, &arm_body_ty), (Ty::Error, _) | (_, Ty::Error)) =>
            {
                ctx.error(TypeError::MatchArmMismatch {
                    first_ty: ref_ty.clone(),
                    found_ty: arm_body_ty,
                    arm_span: arm.span,
                });
            }
            _ => {}
        }

        if arm_is_catchall && catchall_span.is_none() {
            catchall_span = Some(arm.span);
        }
    }

    exhaustiveness_check(&subject_ty, span, catchall_span.is_some(), &covered, true_covered, false_covered, ctx);

    first_arm_ty.unwrap_or(Ty::Error)
}

/// Check a pattern against the expected subject type. Introduces bindings into `env`
/// for the current arm scope. Returns `true` if the pattern is an unconditional catch-all
/// (wildcard `_` or a bare binding name) — used by exhaustiveness tracking.
fn check_pattern(
    pattern: &Pattern<'_>,
    subject_ty: &Ty,
    ctx: &mut InferCtx,
    env: &mut Env,
    covered: &mut HashSet<String>,
    true_covered: &mut bool,
    false_covered: &mut bool,
) -> bool {
    match pattern {
        Pattern::Wildcard(_) => true,

        Pattern::Name(name, _name_span) => {
            match subject_ty {
                Ty::Named(enum_name) if ctx.enum_defs.contains_key(enum_name.as_str()) => {
                    let info = &ctx.enum_defs[enum_name.as_str()];
                    let variant_payload = info.variants.get(*name).map(|v| v.is_empty());
                    match variant_payload {
                        Some(true) => {
                            // Unit variant pattern — covers this variant.
                            covered.insert((*name).to_string());
                            false
                        }
                        Some(false) => {
                            // Tuple variant used without payload pattern (e.g. `Circle` where
                            // `Circle(f64)` expected). Count it covered to suppress cascade.
                            let enum_name_s = enum_name.clone();
                            ctx.error(TypeError::TupleVariantMissingPatternPayload {
                                enum_name: enum_name_s,
                                variant_name: (*name).to_string(),
                                span: pattern.span(),
                            });
                            covered.insert((*name).to_string());
                            false
                        }
                        None => {
                            // Not a known variant of this enum → binding that catches everything.
                            env.define(name, subject_ty.clone());
                            true
                        }
                    }
                }
                _ => {
                    // Non-enum subject → always a binding catch-all.
                    env.define(name, subject_ty.clone());
                    true
                }
            }
        }

        Pattern::Constructor { name, name_span, sub_patterns, span } => {
            match subject_ty {
                Ty::Named(enum_name) if ctx.enum_defs.contains_key(enum_name.as_str()) => {
                    // Read what we need from the immutable borrow before any &mut ctx calls.
                    // Clone payload types only on the success path (where they are iterated);
                    // clone `available` only in the not-found branch (error path only).
                    let info = &ctx.enum_defs[enum_name.as_str()];
                    let enum_name_s = enum_name.clone();
                    match info.variants.get(*name).map(|v| (v.is_empty(), v.len())) {
                        None => {
                            let available = info.variant_order.clone();
                            ctx.error(TypeError::PatternVariantNotFound {
                                enum_name: enum_name_s,
                                variant_name: (*name).to_string(),
                                span: *name_span,
                                available,
                            });
                        }
                        Some((true, _)) => {
                            // Unit variant used with constructor syntax — parentheses not valid.
                            ctx.error(TypeError::UnitVariantInConstructorPattern {
                                enum_name: enum_name_s,
                                variant_name: (*name).to_string(),
                                span: *span,
                            });
                        }
                        Some((false, expected_len)) if expected_len != sub_patterns.len() => {
                            ctx.error(TypeError::PatternArgCountMismatch {
                                enum_name: enum_name_s,
                                variant_name: (*name).to_string(),
                                expected: expected_len,
                                found: sub_patterns.len(),
                                span: *span,
                            });
                        }
                        Some((false, _)) => {
                            // Clone payload types now that we know counts match.
                            let payload_tys =
                                ctx.enum_defs[enum_name.as_str()].variants[*name].clone();
                            for (sub, payload_ty) in sub_patterns.iter().zip(payload_tys.iter()) {
                                // Sub-patterns are not enum-level variants; ignore their coverage.
                                let mut _sc = HashSet::new();
                                let mut _st = false;
                                let mut _sf = false;
                                check_pattern(sub, payload_ty, ctx, env,
                                              &mut _sc, &mut _st, &mut _sf);
                            }
                            covered.insert((*name).to_string());
                        }
                    }
                }
                _ => {
                    ctx.error(TypeError::PatternTypeMismatch {
                        subject_ty: subject_ty.clone(),
                        span: *span,
                    });
                }
            }
            false // constructor pattern is never an unconditional catch-all
        }

        Pattern::Literal(lit, lit_span) => {
            let lit_ty = infer_literal(lit);
            // For Ref-wrapped Str, compare unwrapped.
            let subject_base = match subject_ty {
                Ty::Ref { inner, .. } => inner.as_ref(),
                other => other,
            };
            if !matches!(lit_ty, Ty::Error) && &lit_ty != subject_base && subject_ty != &Ty::Error {
                ctx.error(TypeError::PatternTypeMismatch {
                    subject_ty: subject_ty.clone(),
                    span: *lit_span,
                });
            }
            // Track bool literal coverage for exhaustiveness.
            if let Literal::Bool(b) = lit {
                if *b { *true_covered = true; } else { *false_covered = true; }
            }
            false // literal pattern is never a catch-all
        }

        Pattern::Or(pats, _span) => {
            let mut any_catchall = false;
            for p in pats {
                if check_pattern(p, subject_ty, ctx, env, covered, true_covered, false_covered) {
                    any_catchall = true;
                }
            }
            any_catchall
        }
    }
}

fn exhaustiveness_check(
    subject_ty: &Ty,
    match_span: Span,
    has_catchall: bool,
    covered: &HashSet<String>,
    true_covered: bool,
    false_covered: bool,
    ctx: &mut InferCtx,
) {
    if has_catchall {
        return;
    }
    match subject_ty {
        Ty::Named(enum_name) if ctx.enum_defs.contains_key(enum_name.as_str()) => {
            let missing: Vec<String> = ctx.enum_defs[enum_name.as_str()]
                .variant_order
                .iter()
                .filter(|v| !covered.contains(v.as_str()))
                .cloned()
                .collect();
            if !missing.is_empty() {
                ctx.error(TypeError::NonExhaustiveMatch { missing, span: match_span });
            }
        }
        Ty::Bool => {
            let mut missing = Vec::new();
            if !true_covered  { missing.push("true".to_string()); }
            if !false_covered { missing.push("false".to_string()); }
            if !missing.is_empty() {
                ctx.error(TypeError::NonExhaustiveMatch { missing, span: match_span });
            }
        }
        Ty::Error => {}
        _ => {
            // Open type (i32, f64, Str, Char, etc.) — cannot enumerate; wildcard required.
            ctx.error(TypeError::NonExhaustiveMatch {
                missing: vec!["_".to_string()],
                span: match_span,
            });
        }
    }
}
