use crate::ast::{Expr, Stmt};
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::Ty;

// ─── Statement inference ──────────────────────────────────────────────────────

pub(super) fn infer_stmt(stmt: &Stmt<'_>, return_ty: &Ty, ctx: &mut InferCtx, env: &mut Env) {
    match stmt {
        Stmt::Let { name, ty, init, span, .. } => {
            let init_ty = super::expr::infer_expr(init, ctx, env);

            // FieldOwnNonCopy: `let x = obj.field` when struct is non-Copy (§23).
            if let Expr::Field { object, field, field_span, .. } = init.as_ref() {
                if super::check_field_own_non_copy(object, field, *field_span, &init_ty, ctx) {
                    env.define(name, Ty::Error);
                    return;
                }
            }

            let binding_ty = if let Some(ann) = ty {
                let ann_ty = super::convert::ast_ty_to_ty(ann, &[], None, *span, ctx);
                super::check_types(&ann_ty, &init_ty, *span, ctx, || {
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
            let ann_ty = super::convert::ast_ty_to_ty(ty, &[], None, *span, ctx);
            let init_ty = super::expr::infer_expr(init, ctx, env);
            super::check_types(&ann_ty, &init_ty, *span, ctx, || {
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
                Some(expr) => super::expr::infer_expr(expr, ctx, env),
                None => Ty::Unit,
            };

            // FieldOwnNonCopy: `return obj.field` when struct is non-Copy (§23).
            if let Some(expr) = value {
                if let Expr::Field { object, field, field_span, .. } = expr.as_ref() {
                    if super::check_field_own_non_copy(object, field, *field_span, &ret_ty, ctx) {
                        return;
                    }
                }
            }

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
                super::expr::infer_expr(target, ctx, env);
                super::expr::infer_expr(value, ctx, env);
                super::defer(ctx, "compound assignment operator type checking", *span);
                return;
            }

            // FieldWriteViaSharedRef: detect `(&T).field = ...` before full infer.
            if let Expr::Field { object, field, span: field_span, .. } = target.as_ref() {
                let obj_ty = super::expr::infer_expr(object, ctx, env);
                if let Ty::Ref { mutable: false, inner, .. } = &obj_ty {
                    let type_name = match inner.as_ref() {
                        Ty::Named(n) => n.clone(),
                        t => format!("{t}"),
                    };
                    ctx.error(TypeError::FieldWriteViaSharedRef {
                        type_name,
                        field_name: field.to_string(),
                        span: *field_span,
                    });
                    super::expr::infer_expr(value, ctx, env);
                    return;
                }
                // Mutable ref or owned: fall through to normal path.
                // infer_expr(object) result is already recorded; infer_field_access
                // will call infer_expr(object) again — record() is idempotent.
            }

            let target_ty = super::expr::infer_expr(target, ctx, env);
            let value_ty = super::expr::infer_expr(value, ctx, env);
            super::check_types(&target_ty, &value_ty, *span, ctx, || {
                Some(format!(
                    "assignment: left-hand side has type `{target_ty:?}`, \
                     right-hand side has type `{value_ty:?}` — they must match"
                ))
            });
        }

        Stmt::Expr { expr, .. } => {
            // Result discarded; type-check for side effects only.
            super::expr::infer_expr(expr, ctx, env);
        }

        // Break/continue carry no type information in phase 1.
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}
