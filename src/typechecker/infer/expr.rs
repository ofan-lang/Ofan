use std::collections::HashMap;
use crate::ast::{Expr, Literal, StructFieldInit};
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
        Expr::Literal(lit, _) => infer_literal(lit),

        // ── Identifier ───────────────────────────────────────────────────────
        Expr::Ident(name, span) => {
            if let Some(ty) = env.lookup(name) {
                return ty.clone();
            }
            // Also check if it's a zero-argument function reference used as value.
            // (Rare, but syntactically possible: `let f = my_fn;`)
            // For phase 1 treat function idents as their return type only when the
            // name is unambiguously a function and not a local.
            if let Some((sig, _)) = ctx.fn_sigs.get(*name) {
                return sig.return_ty.clone();
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
            // Use Ty::Unit as return_ty placeholder — nested blocks don't carry a
            // function return type; `return` in a block expression is handled by the
            // enclosing function context (passed down through infer_stmt).
            super::infer_block(block, &Ty::Unit, ctx, env)
        }

        // ── If expression ─────────────────────────────────────────────────────
        Expr::If { condition, then_block, else_branch, span } => {
            let cond_ty = infer_expr(condition, ctx, env);
            if !matches!(cond_ty, Ty::Bool | Ty::Error) {
                ctx.error(TypeError::NonBoolCondition { found: cond_ty, span: *span });
            }
            let then_ty = super::infer_block(then_block, &Ty::Unit, ctx, env);
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
            super::infer_block(body, &Ty::Unit, ctx, env);
            Ty::Unit
        }

        // ── Loop expression ───────────────────────────────────────────────────
        Expr::Loop { body, span: _ } => {
            // Phase 1: treat as unit. Break-value type inference is deferred.
            super::infer_block(body, &Ty::Unit, ctx, env);
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

        // PHASE2: pattern variable binding + exhaustiveness
        Expr::Match { span, .. } => super::defer(ctx, "match expressions", *span),

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
