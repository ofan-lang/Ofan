use crate::ast::{BinOp, Expr, UnaryOp};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::Ty;

// ─── Unary op typing ─────────────────────────────────────────────────────────

pub(super) fn infer_unary(
    op: UnaryOp,
    expr: &Expr<'_>,
    span: Span,
    ctx: &mut InferCtx,
    env: &mut Env,
) -> Ty {
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
