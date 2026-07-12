use crate::ast::{
    Ast, BinOp, Block, Expr, FunctionDef, Item, Literal, RefRegion, Stmt, Type, UnaryOp,
};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{FnSig, Region, Ty};
use crate::typechecker::InferResult;

// ─── Public entry point ───────────────────────────────────────────────────────

pub(crate) fn run(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    let mut ctx = InferCtx::new();

    // Pass 1: collect all function signatures before checking any body.
    // This allows mutual recursion and forward references.
    for item in &ast.items {
        let Item::Function(f) = item;
        collect_fn_sig(f, &mut ctx);
    }

    // Pass 2: check each function body.
    let mut env = Env::new();
    for item in &ast.items {
        let Item::Function(f) = item;
        infer_fn(f, &mut ctx, &mut env);
    }

    if ctx.has_fatal_errors() {
        // Return only fatal errors; deferred are secondary noise when the program
        // has real type errors.
        Err(ctx.errors.into_iter().filter(TypeError::is_fatal).collect())
    } else {
        let deferred = ctx.errors; // only Deferred remain when no fatals
        Ok(InferResult { type_map: ctx.type_map, deferred })
    }
}

// ─── Pass 1: signature collection ────────────────────────────────────────────

fn collect_fn_sig(f: &FunctionDef<'_>, ctx: &mut InferCtx) {
    let is_generic = !f.generic_params.is_empty();
    let params: Vec<Ty> = f
        .params
        .iter()
        .map(|p| ast_ty_to_ty(&p.ty, &f.generic_params, p.span, ctx))
        .collect();
    let return_ty = f
        .return_ty
        .as_ref()
        .map(|t| ast_ty_to_ty(t, &f.generic_params, f.span, ctx))
        .unwrap_or(Ty::Unit);
    ctx.fn_sigs.insert(f.name.to_string(), FnSig { params, return_ty, is_generic });
}

// ─── Pass 2: function body checking ──────────────────────────────────────────

fn infer_fn(f: &FunctionDef<'_>, ctx: &mut InferCtx, env: &mut Env) {
    env.push_scope();

    for param in &f.params {
        let ty = bind_param(param.name, &param.ty, &f.generic_params, param.span, ctx, env);
        env.define(param.name, ty);
    }

    let declared_return = f
        .return_ty
        .as_ref()
        .map(|t| ast_ty_to_ty(t, &f.generic_params, f.span, ctx))
        .unwrap_or(Ty::Unit);

    let body_ty = infer_block(&f.body, &declared_return, ctx, env);

    // Check that the body's tail type matches the declared return type.
    // `return` statements are checked individually as they are encountered.
    if !matches!(body_ty, Ty::Error) && !matches!(declared_return, Ty::Error)
        && body_ty != declared_return
    {
        ctx.error(TypeError::ReturnMismatch {
            expected: declared_return,
            found: body_ty,
            span: f.body.span,
            suggestion: Some(format!(
                "function `{}` body must evaluate to the declared return type",
                f.name
            )),
        });
    }

    env.pop_scope();
}

/// Bind a parameter name to its type. Handles `self`/`&self`/`&mut self` receivers.
///
/// ⚠ METHOD/SELF CONTACT: `self` params require `impl` context that doesn't exist
/// in phase 1. We bind them to `Ty::Error` and emit a `Deferred` diagnostic so
/// inference can continue into the body without panicking.
fn bind_param(
    name: &str,
    ty: &Type<'_>,
    generic_params: &[&str],
    span: Span,
    ctx: &mut InferCtx,
    _env: &mut Env,
) -> Ty {
    // ⚠ METHOD/SELF CONTACT: SelfTy and Ref { inner: SelfTy } arise from
    // `self`, `&self`, `&mut self` parameter syntax (parsed in item.rs).
    let is_self_receiver = matches!(ty, Type::SelfTy(_))
        || matches!(ty, Type::Ref { inner, .. } if matches!(inner.as_ref(), Type::SelfTy(_)));
    if is_self_receiver {
        ctx.error(TypeError::Deferred {
            feature: "self receiver — requires impl block design",
            span,
        });
        return Ty::Error;
    }
    let _ = name;
    ast_ty_to_ty(ty, generic_params, span, ctx)
}

// ─── Block inference ──────────────────────────────────────────────────────────

fn infer_block(block: &Block<'_>, return_ty: &Ty, ctx: &mut InferCtx, env: &mut Env) -> Ty {
    env.push_scope();

    for stmt in &block.stmts {
        // All stmts in Block::stmts have has_semicolon: true (invariant from PR #20).
        // Their value is discarded; we type-check for side-effects and bindings only.
        infer_stmt(stmt, return_ty, ctx, env);
    }

    let tail_ty = match &block.tail {
        Some(expr) => infer_expr(expr, ctx, env),
        None => Ty::Unit,
    };

    ctx.record(block.span, tail_ty.clone());
    env.pop_scope();
    tail_ty
}

// ─── Statement inference ──────────────────────────────────────────────────────

fn infer_stmt(stmt: &Stmt<'_>, return_ty: &Ty, ctx: &mut InferCtx, env: &mut Env) {
    match stmt {
        Stmt::Let { name, ty, init, span, .. } => {
            let init_ty = infer_expr(init, ctx, env);
            let binding_ty = if let Some(ann) = ty {
                let ann_ty = ast_ty_to_ty(ann, &[], *span, ctx);
                check_types(&ann_ty, &init_ty, *span, ctx, || {
                    Some(format!(
                        "variable `{name}` is annotated as `{ann_ty:?}` \
                         but the initializer has type `{init_ty:?}` — \
                         change the annotation or the initializer"
                    ))
                });
                ann_ty
            } else {
                init_ty
            };
            env.define(name, binding_ty);
        }

        Stmt::Const { name, ty, init, span, .. } => {
            let ann_ty = ast_ty_to_ty(ty, &[], *span, ctx);
            let init_ty = infer_expr(init, ctx, env);
            check_types(&ann_ty, &init_ty, *span, ctx, || {
                Some(format!(
                    "constant `{name}` is annotated as `{ann_ty:?}` \
                     but the initializer has type `{init_ty:?}` — \
                     change the annotation or the initializer"
                ))
            });
            env.define(name, ann_ty);
        }

        Stmt::Return { value, span } => {
            let ret_ty = match value {
                Some(expr) => infer_expr(expr, ctx, env),
                None => Ty::Unit,
            };
            if !matches!(ret_ty, Ty::Error)
                && !matches!(return_ty, Ty::Error)
                && &ret_ty != return_ty
            {
                ctx.error(TypeError::ReturnMismatch {
                    expected: return_ty.clone(),
                    found: ret_ty,
                    span: *span,
                    suggestion: Some(format!(
                        "function expects `{return_ty:?}` — \
                         adjust the return expression or the function signature"
                    )),
                });
            }
        }

        Stmt::Assign { target, op, value, span } => {
            // Compound assignments (+=, -=, etc.) require knowing the operator's
            // typing rule (e.g. += requires numeric); defer to avoid silently
            // accepting `x += true` as type-correct.
            // PHASE2: implement compound-assign operator type checking.
            if op.is_some() {
                infer_expr(target, ctx, env);
                infer_expr(value, ctx, env);
                defer(ctx, "compound assignment operator type checking", *span);
                return;
            }
            let target_ty = infer_expr(target, ctx, env);
            let value_ty = infer_expr(value, ctx, env);
            check_types(&target_ty, &value_ty, *span, ctx, || {
                Some(format!(
                    "assignment: left-hand side has type `{target_ty:?}`, \
                     right-hand side has type `{value_ty:?}` — they must match"
                ))
            });
        }

        Stmt::Expr { expr, .. } => {
            // Result discarded; type-check for side effects only.
            infer_expr(expr, ctx, env);
        }

        // Break/continue carry no type information in phase 1.
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

// ─── Expression inference ─────────────────────────────────────────────────────

fn infer_expr(expr: &Expr<'_>, ctx: &mut InferCtx, env: &mut Env) -> Ty {
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
            if let Some(sig) = ctx.fn_sigs.get(*name) {
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
        Expr::Unary { op, expr, span } => infer_unary(*op, expr, *span, ctx, env),

        // ── Binary ───────────────────────────────────────────────────────────
        Expr::Binary { op, left, right, span } => {
            infer_binary(*op, left, right, *span, ctx, env)
        }

        // ── Function call ─────────────────────────────────────────────────────
        Expr::Call { callee, args, span } => infer_call(callee, args, *span, ctx, env),

        // ── Block expression ──────────────────────────────────────────────────
        Expr::Block(block) => {
            // Use Ty::Unit as return_ty placeholder — nested blocks don't carry a
            // function return type; `return` in a block expression is handled by the
            // enclosing function context (passed down through infer_stmt).
            infer_block(block, &Ty::Unit, ctx, env)
        }

        // ── If expression ─────────────────────────────────────────────────────
        Expr::If { condition, then_block, else_branch, span } => {
            let cond_ty = infer_expr(condition, ctx, env);
            if !matches!(cond_ty, Ty::Bool | Ty::Error) {
                ctx.error(TypeError::NonBoolCondition { found: cond_ty, span: *span });
            }
            let then_ty = infer_block(then_block, &Ty::Unit, ctx, env);
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
            infer_block(body, &Ty::Unit, ctx, env);
            Ty::Unit
        }

        // ── Loop expression ───────────────────────────────────────────────────
        Expr::Loop { body, span: _ } => {
            // Phase 1: treat as unit. Break-value type inference is deferred.
            infer_block(body, &Ty::Unit, ctx, env);
            Ty::Unit
        }

        // ── Deferred expressions ──────────────────────────────────────────────

        // ⚠ METHOD/SELF CONTACT: MethodCall requires impl block + method table.
        Expr::MethodCall { span, .. } => {
            defer(ctx, "method calls — requires impl block design", *span)
        }

        // ⚠ METHOD/SELF CONTACT: Field access requires struct field table.
        Expr::Field { span, .. } => {
            defer(ctx, "field access — requires struct design", *span)
        }

        // PHASE2: cast rules not yet spec'd
        Expr::Cast { span, .. } => defer(ctx, "cast (as) — rules not yet spec'd", *span),

        // PHASE2: requires Checked<T, E> impl
        Expr::Propagate { span, .. } => {
            defer(ctx, "? operator — requires Checked<T, E> design", *span)
        }

        // PHASE2: requires iterator trait
        Expr::For { span, .. } => defer(ctx, "for loops — requires iterator trait", *span),

        // PHASE2: pattern variable binding + exhaustiveness
        Expr::Match { span, .. } => defer(ctx, "match expressions", *span),
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

// ─── Unary op typing ─────────────────────────────────────────────────────────

fn infer_unary(
    op: UnaryOp,
    expr: &Expr<'_>,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    let operand_ty = infer_expr(expr, ctx, env);
    match op {
        UnaryOp::Neg => match &operand_ty {
            Ty::I32 => Ty::I32,
            Ty::F64 => Ty::F64,
            Ty::Error => Ty::Error,
            other => {
                ctx.error(TypeError::Mismatch {
                    expected: Ty::I32,
                    found: other.clone(),
                    span,
                    suggestion: Some(
                        "unary `-` requires a numeric type (`i32` or `f64`)".to_string(),
                    ),
                });
                Ty::Error
            }
        },
        UnaryOp::Not => match &operand_ty {
            Ty::Bool => Ty::Bool,
            Ty::Error => Ty::Error,
            other => {
                ctx.error(TypeError::Mismatch {
                    expected: Ty::Bool,
                    found: other.clone(),
                    span,
                    suggestion: Some("unary `!` requires a `bool` operand".to_string()),
                });
                Ty::Error
            }
        },
        UnaryOp::BitNot => match &operand_ty {
            Ty::I32 => Ty::I32,
            Ty::Error => Ty::Error,
            other => {
                ctx.error(TypeError::Mismatch {
                    expected: Ty::I32,
                    found: other.clone(),
                    span,
                    suggestion: Some("unary `~` requires an `i32` operand".to_string()),
                });
                Ty::Error
            }
        },
        UnaryOp::Borrow => {
            if matches!(operand_ty, Ty::Error) { return Ty::Error; }
            Ty::Ref { mutable: false, region: None, inner: Box::new(operand_ty) }
        }
        UnaryOp::BorrowMut => {
            if matches!(operand_ty, Ty::Error) { return Ty::Error; }
            Ty::Ref { mutable: true, region: None, inner: Box::new(operand_ty) }
        }
    }
}

// ─── Binary op typing ────────────────────────────────────────────────────────

fn infer_binary(
    op: BinOp,
    left: &Expr<'_>,
    right: &Expr<'_>,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Fallback operator requires Option<T> — deferred.
    if matches!(op, BinOp::Fallback) {
        return defer(ctx, "`?:` operator — requires Option<T> design", span);
    }

    let lhs = infer_expr(left, ctx, env);
    let rhs = infer_expr(right, ctx, env);

    // Suppress cascade errors: if either operand failed, don't add more noise.
    if matches!(lhs, Ty::Error) || matches!(rhs, Ty::Error) {
        return Ty::Error;
    }

    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            match (&lhs, &rhs) {
                (Ty::I32, Ty::I32) => Ty::I32,
                (Ty::F64, Ty::F64) => Ty::F64,
                _ => {
                    ctx.error(TypeError::Mismatch {
                        expected: lhs.clone(),
                        found: rhs,
                        span,
                        suggestion: Some(
                            "arithmetic operators require both operands to be the same \
                             numeric type (`i32` or `f64`)"
                                .to_string(),
                        ),
                    });
                    Ty::Error
                }
            }
        }

        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            match (&lhs, &rhs) {
                (Ty::I32, Ty::I32) => Ty::I32,
                _ => {
                    ctx.error(TypeError::Mismatch {
                        expected: Ty::I32,
                        found: if lhs != Ty::I32 { lhs } else { rhs },
                        span,
                        suggestion: Some(
                            "bitwise operators require `i32` operands".to_string(),
                        ),
                    });
                    Ty::Error
                }
            }
        }

        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
            if lhs == rhs {
                Ty::Bool
            } else {
                ctx.error(TypeError::Mismatch {
                    expected: lhs,
                    found: rhs,
                    span,
                    suggestion: Some(
                        "comparison operators require both operands to have the same type"
                            .to_string(),
                    ),
                });
                Ty::Error
            }
        }

        BinOp::And | BinOp::Or => match (&lhs, &rhs) {
            (Ty::Bool, Ty::Bool) => Ty::Bool,
            _ => {
                let bad = if lhs != Ty::Bool { lhs } else { rhs };
                ctx.error(TypeError::Mismatch {
                    expected: Ty::Bool,
                    found: bad,
                    span,
                    suggestion: Some(
                        "logical `&&` / `||` require `bool` operands".to_string(),
                    ),
                });
                Ty::Error
            }
        },

        BinOp::Fallback => unreachable!("handled above"),
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
            return defer(ctx, "non-identifier callees (function pointers, closures)", span);
        }
    };

    // Look up in environment first (local binding shadows function name).
    if env.lookup(name).is_some() {
        // A local variable is being called as a function — not supported in phase 1.
        for arg in args {
            infer_expr(arg, ctx, env);
        }
        return defer(ctx, "calling local variables as functions (closures/fn pointers)", span);
    }

    let sig = match ctx.fn_sigs.get(name).cloned() {
        Some(s) => s,
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
        return defer(
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

// ─── AST type → internal Ty ───────────────────────────────────────────────────

pub(crate) fn ast_ty_to_ty(
    ty: &Type<'_>,
    generic_params: &[&str],
    span: Span,
    ctx: &mut InferCtx,
) -> Ty {
    match ty {
        Type::Named { name, args, .. } => {
            if args.is_empty() {
                match *name {
                    "i32" => return Ty::I32,
                    "f64" => return Ty::F64,
                    "bool" => return Ty::Bool,
                    "char" => return Ty::Char,
                    "str" => return Ty::Str,
                    "unit" => return Ty::Unit,
                    n if generic_params.contains(&n) => return Ty::Param(n.to_string()),
                    // No struct/enum definitions in phase 1 — any unknown type name is
                    // unresolvable. Defer so it surfaces in InferResult::deferred rather
                    // than silently reaching codegen as Ty::Named.
                    n => return defer(ctx, "user-defined type — struct/enum design pending", span),
                }
            }
            // Generic instantiation (e.g. `Option<i32>`): deferred in phase 1.
            return defer(ctx, "generic type instantiation", span);
        }
        Type::Ref { mutable, region, inner, .. } => {
            let inner_ty = ast_ty_to_ty(inner, generic_params, span, ctx);
            let region = region.as_ref().map(ast_region_to_region);
            Ty::Ref { mutable: *mutable, region, inner: Box::new(inner_ty) }
        }
        // ⚠ METHOD/SELF CONTACT: `Self` type requires `impl` block context.
        Type::SelfTy(self_span) => {
            ctx.error(TypeError::Deferred {
                feature: "Self type — requires impl block design",
                span: *self_span,
            });
            Ty::Error
        }
    }
}

fn ast_region_to_region(r: &RefRegion<'_>) -> Region {
    match r {
        RefRegion::Named(name) => Region::Named(name.to_string()),
        RefRegion::Static => Region::Static,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Emit a `Deferred` diagnostic and return `Ty::Error`.
/// Inference continues — callers treat `Ty::Error` as a cascade-suppressing sentinel.
fn defer(ctx: &mut InferCtx, feature: &'static str, span: Span) -> Ty {
    ctx.error(TypeError::Deferred { feature, span });
    Ty::Error
}

/// Check that `found` matches `expected`, suppressing cascades when either is `Ty::Error`.
/// `suggestion_fn` is called lazily only when an error is emitted (avoids allocation
/// for the common success path).
fn check_types<F>(expected: &Ty, found: &Ty, span: Span, ctx: &mut InferCtx, suggestion_fn: F)
where
    F: FnOnce() -> Option<String>,
{
    if matches!(expected, Ty::Error) || matches!(found, Ty::Error) {
        return;
    }
    if expected != found {
        ctx.error(TypeError::Mismatch {
            expected: expected.clone(),
            found: found.clone(),
            span,
            suggestion: suggestion_fn(),
        });
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::typechecker::{self, Ty};

    /// Parse a function source string and run inference. Returns the inferred
    /// return type of the first (and only) function on success.
    fn check_fn(src: &str) -> Result<Ty, Vec<TypeError>> {
        let tokens = Lexer::new(src).lex().expect("lex failed in test");
        let ast = Parser::new(tokens).parse().expect("parse failed in test");
        let result = typechecker::infer(&ast)?;
        // Return the type of the tail expression of the first function's body.
        // Fall back to Unit if no tail (void function).
        let Item::Function(f) = &ast.items[0];
        let tail_span = f
            .body
            .tail
            .as_ref()
            .map(|e| e.span())
            .unwrap_or(f.body.span);
        Ok(result.type_of(tail_span).cloned().unwrap_or(Ty::Unit))
    }

    /// Parse and infer; return all fatal errors (ignores Deferred).
    fn check_fn_errors(src: &str) -> Vec<TypeError> {
        let tokens = Lexer::new(src).lex().expect("lex failed in test");
        let ast = Parser::new(tokens).parse().expect("parse failed in test");
        match typechecker::infer(&ast) {
            Ok(_) => vec![],
            Err(errs) => errs.into_iter().filter(TypeError::is_fatal).collect(),
        }
    }

    // ── Correct programs ──────────────────────────────────────────────────────

    #[test]
    fn infer_void_fn() {
        assert_eq!(check_fn("fn greet() { }").unwrap(), Ty::Unit);
    }

    #[test]
    fn infer_i32_literal_return() {
        assert_eq!(check_fn("fn answer() -> i32 { 42 }").unwrap(), Ty::I32);
    }

    #[test]
    fn infer_f64_literal_return() {
        assert_eq!(check_fn("fn pi() -> f64 { 3.14 }").unwrap(), Ty::F64);
    }

    #[test]
    fn infer_bool_literal_return() {
        assert_eq!(check_fn("fn yes() -> bool { true }").unwrap(), Ty::Bool);
    }

    #[test]
    fn infer_param_passthrough() {
        assert_eq!(check_fn("fn id(x: bool) -> bool { x }").unwrap(), Ty::Bool);
    }

    #[test]
    fn infer_binary_arithmetic() {
        assert_eq!(check_fn("fn double(n: i32) -> i32 { n * 2 }").unwrap(), Ty::I32);
    }

    #[test]
    fn infer_comparison_returns_bool() {
        assert_eq!(check_fn("fn gt(a: i32, b: i32) -> bool { a > b }").unwrap(), Ty::Bool);
    }

    #[test]
    fn infer_logical_returns_bool() {
        assert_eq!(
            check_fn("fn both(a: bool, b: bool) -> bool { a && b }").unwrap(),
            Ty::Bool
        );
    }

    #[test]
    fn infer_unary_neg() {
        assert_eq!(check_fn("fn neg(x: i32) -> i32 { -x }").unwrap(), Ty::I32);
    }

    #[test]
    fn infer_unary_not() {
        assert_eq!(check_fn("fn inv(b: bool) -> bool { !b }").unwrap(), Ty::Bool);
    }

    #[test]
    fn infer_let_binding_with_annotation() {
        assert_eq!(
            check_fn("fn seq() -> i32 { let x: i32 = 5; x }").unwrap(),
            Ty::I32
        );
    }

    #[test]
    fn infer_let_binding_inferred() {
        assert_eq!(
            check_fn("fn seq() -> i32 { let x = 5; x }").unwrap(),
            Ty::I32
        );
    }

    #[test]
    fn infer_if_expression() {
        assert_eq!(
            check_fn("fn branch(b: bool) -> i32 { if b { 1 } else { 2 } }").unwrap(),
            Ty::I32
        );
    }

    #[test]
    fn infer_if_no_else_is_unit() {
        assert_eq!(
            check_fn("fn maybe(b: bool) { if b { let _x: i32 = 1; } }").unwrap(),
            Ty::Unit
        );
    }

    #[test]
    fn infer_free_function_call() {
        let src = "fn double(n: i32) -> i32 { n * 2 } fn quad(n: i32) -> i32 { double(double(n)) }";
        let tokens = Lexer::new(src).lex().expect("lex failed");
        let ast = Parser::new(tokens).parse().expect("parse failed");
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn infer_while_is_unit() {
        assert_eq!(
            check_fn("fn count(n: i32) { let mut x: i32 = 0; while x < n { x = x + 1; } }").unwrap(),
            Ty::Unit
        );
    }

    // ── Type errors ───────────────────────────────────────────────────────────

    #[test]
    fn error_return_mismatch() {
        let errs = check_fn_errors("fn bad() -> i32 { true }");
        assert!(!errs.is_empty());
        assert!(matches!(
            errs[0],
            TypeError::ReturnMismatch { expected: Ty::I32, found: Ty::Bool, .. }
        ));
    }

    #[test]
    fn error_undefined_variable() {
        let errs = check_fn_errors("fn bad() -> i32 { missing }");
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], TypeError::UndefinedVariable { .. }));
    }

    #[test]
    fn error_let_annotation_mismatch() {
        let errs = check_fn_errors("fn bad() -> i32 { let x: bool = 5; x }");
        assert!(!errs.is_empty());
        assert!(matches!(
            errs[0],
            TypeError::Mismatch { expected: Ty::Bool, found: Ty::I32, .. }
        ));
    }

    #[test]
    fn error_non_bool_condition() {
        let errs = check_fn_errors("fn bad(n: i32) -> i32 { if n { 1 } else { 2 } }");
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], TypeError::NonBoolCondition { found: Ty::I32, .. }));
    }

    #[test]
    fn error_branch_type_mismatch() {
        let errs = check_fn_errors(
            "fn bad(b: bool) -> i32 { if b { 1 } else { true } }",
        );
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], TypeError::BranchMismatch { .. }));
    }

    #[test]
    fn error_arg_count_mismatch() {
        let errs = check_fn_errors(
            "fn double(n: i32) -> i32 { n * 2 } fn bad() -> i32 { double(1, 2) }",
        );
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], TypeError::ArgCountMismatch { .. }));
    }

    #[test]
    fn error_undefined_function() {
        let errs = check_fn_errors("fn bad() -> i32 { nope() }");
        assert!(!errs.is_empty());
        assert!(matches!(errs[0], TypeError::UndefinedFunction { .. }));
    }

    // ── Deferred — non-fatal, inference continues ─────────────────────────────

    #[test]
    fn deferred_method_call_non_fatal() {
        // Method call is deferred but must not produce a fatal error.
        let tokens = Lexer::new("fn f(n: i32) { n.abs(); }").lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        // Should succeed (Ok) — Deferred is non-fatal.
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn deferred_field_access_non_fatal() {
        let tokens = Lexer::new("fn f(n: i32) { let _x = n.foo; }").lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }
}
