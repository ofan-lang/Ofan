use crate::ast::{BinOp, Expr, Literal, UnaryOp};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::Ty;

// ─── Compound assignment op checking ─────────────────────────────────────────

/// Type-check a compound assignment `target op= value`.
/// Applies the same operator rules as `infer_binary` but without re-inferring
/// the operand expressions (already inferred by the caller).
/// Cascade-suppressed when either operand is `Ty::Error`.
pub(super) fn check_binary_op_types(
    op: BinOp,
    lhs_ty: &Ty,
    rhs_ty: &Ty,
    span: Span,
    ctx: &mut InferCtx,
) {
    if matches!(lhs_ty, Ty::Error) || matches!(rhs_ty, Ty::Error) {
        return;
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            match (lhs_ty, rhs_ty) {
                (Ty::I32, Ty::I32) | (Ty::F64, Ty::F64) => {}
                _ => ctx.error(TypeError::Mismatch {
                    expected: lhs_ty.clone(),
                    found: rhs_ty.clone(),
                    span,
                    suggestion: Some(
                        "compound arithmetic assignment requires both sides to be the \
                         same numeric type (`i32` or `f64`)"
                            .to_string(),
                    ),
                }),
            }
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            match (lhs_ty, rhs_ty) {
                (Ty::I32, Ty::I32) => {}
                _ => ctx.error(TypeError::Mismatch {
                    expected: Ty::I32,
                    found: if lhs_ty != &Ty::I32 { lhs_ty.clone() } else { rhs_ty.clone() },
                    span,
                    suggestion: Some(
                        "compound bitwise assignment requires `i32` operands".to_string(),
                    ),
                }),
            }
        }
        // Only arithmetic and bitwise ops are valid compound-assignment operators;
        // comparison, logical, and fallback ops cannot appear here from the parser.
        _ => {}
    }
}

// ─── Unary op typing ─────────────────────────────────────────────────────────

pub(super) fn infer_unary(
    op: UnaryOp,
    expr: &Expr<'_>,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // -2147483648 = i32::MIN: the bare literal 2147483648 exceeds i32::MAX and would
    // trigger IntegerOutOfRange, but under negation it is a valid i32. Intercept before
    // infer_expr reaches the literal to suppress the false error.
    // Manual ctx.record is safe: the outer infer_expr wrapper records the full Unary
    // span; recording the inner literal span here is the only side effect the inner
    // infer_expr call would have produced. Nothing is duplicated.
    if matches!(op, UnaryOp::Neg) {
        if let Expr::Literal(Literal::Integer(n), lit_span) = expr {
            if *n == (i32::MAX as i64) + 1 {
                ctx.record(*lit_span, Ty::I32);
                return Ty::I32;
            }
        }
    }
    let operand_ty = super::expr::infer_expr(expr, ctx, env);
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

pub(super) fn infer_binary(
    op: BinOp,
    left: &Expr<'_>,
    right: &Expr<'_>,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
    // Fallback operator requires Option<T> — deferred.
    if matches!(op, BinOp::Fallback) {
        return super::defer(ctx, "`?:` operator — requires Option<T> design", span);
    }

    let lhs = super::expr::infer_expr(left, ctx, env);
    let rhs = super::expr::infer_expr(right, ctx, env);

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
