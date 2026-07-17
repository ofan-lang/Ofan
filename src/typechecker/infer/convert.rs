use crate::ast::{RefRegion, Type};
use crate::lexer::token::Span;
use crate::typechecker::env::InferCtx;
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{Region, Ty};

// ─── AST type → internal Ty ───────────────────────────────────────────────────

pub(super) fn ast_ty_to_ty(
    ty: &Type<'_>,
    generic_params: &[&str],
    impl_type_name: Option<&str>,
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
                    n if ctx.struct_defs.contains_key(n) => return Ty::Named(n.to_string()),
                    _ => return super::defer(ctx, "user-defined type — struct/enum design pending", span),
                }
            }
            // Generic instantiation (e.g. `Option<i32>`): deferred in phase 1.
            super::defer(ctx, "generic type instantiation", span)
        }
        Type::Ref { mutable, region, inner, .. } => {
            let inner_ty = ast_ty_to_ty(inner, generic_params, impl_type_name, span, ctx);
            let region = region.as_ref().map(ast_region_to_region);
            Ty::Ref { mutable: *mutable, region, inner: Box::new(inner_ty) }
        }
        // `Self` resolves to the enclosing impl type when context is available (§18).
        Type::SelfTy(self_span) => {
            if let Some(type_name) = impl_type_name {
                Ty::Named(type_name.to_string())
            } else {
                ctx.error(TypeError::Deferred {
                    feature: "Self type — requires impl block design",
                    span: *self_span,
                });
                Ty::Error
            }
        }
    }
}

fn ast_region_to_region(r: &RefRegion<'_>) -> Region {
    match r {
        RefRegion::Named(name) => Region::Named(name.to_string()),
        RefRegion::Static => Region::Static,
    }
}
