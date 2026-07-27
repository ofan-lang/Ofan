mod stmt;
mod expr;
mod ops;
mod convert;
mod self_access;

use crate::ast::{Ast, Block, CopyMove, EnumDef, Expr, FunctionDef, ImplBlock, Item, Stmt, StructDef, Type};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, EnumInfo, InferCtx, StructInfo};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{FnSig, Ty};
use crate::typechecker::InferResult;

// ─── Public entry point ───────────────────────────────────────────────────────

pub(crate) fn run(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    let mut ctx = InferCtx::new();

    // Sub-pass 1a: register struct names (enables forward references in fields).
    for item in &ast.items {
        if let Item::Struct(def) = item {
            collect_struct_name(def, &mut ctx);
        }
    }

    // Sub-pass 1b: register enum names (enables forward references in variant fields).
    for item in &ast.items {
        if let Item::Enum(def) = item {
            collect_enum_name(def, &mut ctx);
        }
    }

    // Sub-pass 1c: populate struct field types (all type names known from 1a/1b).
    for item in &ast.items {
        if let Item::Struct(def) = item {
            collect_struct_fields(def, &mut ctx);
        }
    }

    // Sub-pass 1d: populate enum variant types and build variant_to_enum index.
    for item in &ast.items {
        if let Item::Enum(def) = item {
            collect_enum_variants(def, &mut ctx);
        }
    }

    // Sub-pass 1e: collect fn/impl signatures.
    for item in &ast.items {
        match item {
            Item::Function(f) => collect_fn_sig(f, &mut ctx),
            Item::Impl(block) => collect_impl_sigs(block, &mut ctx),
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }

    // Pass 2: check each function/method body.
    let mut env = Env::new();
    for item in &ast.items {
        match item {
            Item::Function(f) => infer_fn(f, &mut ctx, &mut env),
            Item::Impl(block) => infer_impl_methods(block, &mut ctx, &mut env),
            Item::Struct(_) | Item::Enum(_) => {}
        }
    }

    if ctx.has_fatal_errors() {
        // Return only fatal errors; deferred are secondary noise when the program
        // has real type errors.
        Err(ctx.errors.into_iter().filter(TypeError::is_fatal).collect())
    } else {
        let deferred = ctx.errors; // only Deferred remain when no fatals
        Ok(InferResult {
            type_map: ctx.type_map,
            deferred,
            struct_defs: ctx.struct_defs,
            impl_sigs: ctx.impl_sigs,
        })
    }
}

// ─── Pass 1: signature collection ────────────────────────────────────────────

fn collect_fn_sig(f: &FunctionDef<'_>, ctx: &mut InferCtx) {
    let is_generic = !f.generic_params.is_empty();
    let params: Vec<Ty> = f
        .params
        .iter()
        .map(|p| convert::ast_ty_to_ty(&p.ty, &f.generic_params, None, p.span, ctx))
        .collect();
    let return_ty = f
        .return_ty
        .as_ref()
        .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, None, f.span, ctx))
        .unwrap_or(Ty::Unit);
    let sig = FnSig { params, return_ty, is_generic, self_consuming: false };

    if let Some((_, first_span)) = ctx.fn_sigs.get(f.name) {
        ctx.error(TypeError::DuplicateFn {
            name: f.name.to_string(),
            first_span: *first_span,
            duplicate_span: f.name_span,
        });
        return; // keep first definition; don't overwrite
    }
    ctx.fn_sigs.insert(f.name.to_string(), (sig, f.name_span));
}

fn collect_impl_sigs(block: &ImplBlock<'_>, ctx: &mut InferCtx) {
    for f in &block.methods {
        let is_generic = !f.generic_params.is_empty();

        // Strip the self receiver — method calls don't pass self as an explicit argument.
        // Record whether it was `move self` so infer_method_call can reject calling
        // a consuming method through a reference receiver.
        let (self_consuming, param_slice) = match f.params.first() {
            Some(p) if matches!(p.ty, Type::SelfTy(_)) => (p.consuming, &f.params[1..]),
            _ => (false, &f.params[..]),
        };

        let params: Vec<Ty> = param_slice
            .iter()
            .map(|p| convert::ast_ty_to_ty(&p.ty, &f.generic_params, Some(block.type_name), p.span, ctx))
            .collect();
        let return_ty = f
            .return_ty
            .as_ref()
            .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, Some(block.type_name), f.span, ctx))
            .unwrap_or(Ty::Unit);
        let sig = FnSig { params, return_ty, is_generic, self_consuming };

        // Re-borrow each iteration to avoid holding &mut across ctx.error().
        if let Some((_, first_span)) = ctx.impl_sigs
            .get(block.type_name)
            .and_then(|ns| ns.get(f.name))
        {
            ctx.error(TypeError::DuplicateMethod {
                type_name: block.type_name.to_string(),
                method_name: f.name.to_string(),
                first_span: *first_span,
                duplicate_span: f.name_span,
            });
        } else {
            ctx.impl_sigs
                .entry(block.type_name.to_string())
                .or_default()
                .insert(f.name.to_string(), (sig, f.name_span));
        }
    }
}

// ─── Pass 1: struct collection ───────────────────────────────────────────────

fn collect_struct_name(def: &StructDef<'_>, ctx: &mut InferCtx) {
    if let Some(existing) = ctx.struct_defs.get(def.name) {
        ctx.error(TypeError::DuplicateStruct {
            name: def.name.to_string(),
            first_span: existing.name_span,
            duplicate_span: def.name_span,
        });
        return;
    }
    ctx.struct_defs.insert(def.name.to_string(), StructInfo {
        name_span: def.name_span,
        fields: std::collections::HashMap::new(),
        field_order: Vec::new(),
        copy_override: None,
        is_generic: !def.generic_params.is_empty(),
    });
}

fn collect_struct_fields(def: &StructDef<'_>, ctx: &mut InferCtx) {
    if !ctx.struct_defs.contains_key(def.name) {
        return; // duplicate struct — skip field population
    }
    let gp: Vec<&str> = def.generic_params.to_vec();
    let mut fields = std::collections::HashMap::new();
    let mut field_order = Vec::new();
    for f in &def.fields {
        let ty = convert::ast_ty_to_ty(&f.ty, &gp, None, f.span, ctx);
        fields.insert(f.name.to_string(), ty);
        field_order.push(f.name.to_string());
    }
    if let Some(info) = ctx.struct_defs.get_mut(def.name) {
        info.fields = fields;
        info.field_order = field_order;
        info.copy_override = def.copy_move;
    }
}

// ─── Pass 1: enum collection ──────────────────────────────────────────────────

fn collect_enum_name(def: &EnumDef<'_>, ctx: &mut InferCtx) {
    if let Some(existing) = ctx.enum_defs.get(def.name) {
        ctx.error(TypeError::DuplicateEnum {
            name: def.name.to_string(),
            first_span: existing.name_span,
            duplicate_span: def.name_span,
        });
        return;
    }
    ctx.enum_defs.insert(def.name.to_string(), EnumInfo {
        name_span: def.name_span,
        variants: std::collections::HashMap::new(),
        variant_order: Vec::new(),
        copy_override: None,
        is_generic: !def.generic_params.is_empty(),
    });
}

fn collect_enum_variants(def: &EnumDef<'_>, ctx: &mut InferCtx) {
    if !ctx.enum_defs.contains_key(def.name) {
        return; // duplicate enum — skip field population
    }
    let gp: Vec<&str> = def.generic_params.to_vec();
    let mut variants: std::collections::HashMap<String, Vec<crate::typechecker::ty::Ty>> =
        std::collections::HashMap::new();
    let mut variant_order: Vec<String> = Vec::new();
    let mut first_spans: std::collections::HashMap<String, Span> = std::collections::HashMap::new();
    // Collect duplicate errors to emit after the loop (avoids borrow conflicts).
    let mut dup_errors: Vec<(String, Span, Span)> = Vec::new();

    for v in &def.variants {
        if let Some(&first_span) = first_spans.get(v.name) {
            dup_errors.push((v.name.to_string(), first_span, v.name_span));
            continue;
        }
        first_spans.insert(v.name.to_string(), v.name_span);

        let field_tys: Vec<crate::typechecker::ty::Ty> = v.fields.iter()
            .map(|ty| convert::ast_ty_to_ty(ty, &gp, None, v.span, ctx))
            .collect();
        variants.insert(v.name.to_string(), field_tys);
        variant_order.push(v.name.to_string());
    }

    for (vname, first_span, dup_span) in dup_errors {
        ctx.error(TypeError::DuplicateVariant {
            enum_name: def.name.to_string(),
            variant_name: vname,
            first_span,
            duplicate_span: dup_span,
        });
    }

    for vname in &variant_order {
        ctx.variant_to_enum
            .entry(vname.clone())
            .or_default()
            .push(def.name.to_string());
    }

    if let Some(info) = ctx.enum_defs.get_mut(def.name) {
        info.variants = variants;
        info.variant_order = variant_order;
        info.copy_override = def.copy_move;
    }
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
        .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, None, f.span, ctx))
        .unwrap_or(Ty::Unit);

    ctx.current_return_ty.push(declared_return.clone());
    let body_ty = infer_block(&f.body, &declared_return, ctx, env);
    ctx.current_return_ty.pop();

    // FieldOwnNonCopy: detect partial moves in implicit function return (tail expr, §23).
    let tail_owns_non_copy = f.body.tail.as_ref()
        .is_some_and(|tail| check_tail_field_own_non_copy(tail, ctx));

    // Check that the body's tail type matches the declared return type.
    // `return` statements are checked individually as they are encountered.
    // Suppress ReturnMismatch when FieldOwnNonCopy already fired — the ownership
    // error is the root cause; the type error is noise on top of it.
    // Also suppress when the body has no tail but ends with an explicit `return` — the
    // block's Unit tail type is spurious because control flow never reaches the end.
    let ends_with_return = f.body.tail.is_none()
        && matches!(f.body.stmts.last(), Some(Stmt::Return { .. }));
    if !tail_owns_non_copy
        && !ends_with_return
        && !matches!(body_ty, Ty::Error) && !matches!(declared_return, Ty::Error)
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

/// Bind a non-self parameter to its type (called from `infer_fn` for top-level functions).
/// `self`/`move self` params in methods are handled by `infer_method` before this is called.
/// If a top-level `fn` somehow has a `self` param (syntactically odd), defer as before.
fn bind_param(
    name: &str,
    ty: &Type<'_>,
    generic_params: &[&str],
    span: Span,
    ctx: &mut InferCtx,
    _env: &mut Env,
) -> Ty {
    if matches!(ty, Type::SelfTy(_)) {
        ctx.error(TypeError::Deferred {
            feature: "self receiver in top-level fn — only valid inside an impl block",
            span,
        });
        return Ty::Error;
    }
    let _ = name;
    convert::ast_ty_to_ty(ty, generic_params, None, span, ctx)
}

// ─── Pass 2: impl block method body checking ─────────────────────────────────

fn infer_impl_methods(block: &ImplBlock<'_>, ctx: &mut InferCtx, env: &mut Env) {
    for f in &block.methods {
        infer_method(f, block.type_name, ctx, env);
    }
}

fn infer_method(f: &FunctionDef<'_>, impl_type_name: &str, ctx: &mut InferCtx, env: &mut Env) {
    env.push_scope();

    for param in &f.params {
        let ty = if matches!(param.ty, Type::SelfTy(_)) {
            if param.consuming {
                Ty::Named(impl_type_name.to_string())
            } else {
                self_access::infer_self_access_mode(impl_type_name, &f.body, f.name, ctx)
            }
        } else {
            // Non-self param: Self in type position resolves via impl context.
            convert::ast_ty_to_ty(&param.ty, &f.generic_params, Some(impl_type_name), param.span, ctx)
        };
        env.define(param.name, ty);
    }

    let declared_return = f
        .return_ty
        .as_ref()
        .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, Some(impl_type_name), f.span, ctx))
        .unwrap_or(Ty::Unit);

    ctx.current_return_ty.push(declared_return.clone());
    let body_ty = infer_block(&f.body, &declared_return, ctx, env);
    ctx.current_return_ty.pop();

    // FieldOwnNonCopy: detect partial moves in implicit method return (tail expr, §23).
    let tail_owns_non_copy = f.body.tail.as_ref()
        .is_some_and(|tail| check_tail_field_own_non_copy(tail, ctx));

    let ends_with_return = f.body.tail.is_none()
        && matches!(f.body.stmts.last(), Some(Stmt::Return { .. }));
    if !tail_owns_non_copy
        && !ends_with_return
        && !matches!(body_ty, Ty::Error) && !matches!(declared_return, Ty::Error)
        && body_ty != declared_return
    {
        ctx.error(TypeError::ReturnMismatch {
            expected: declared_return,
            found: body_ty,
            span: f.body.span,
            suggestion: Some(format!(
                "method `{}` body must evaluate to the declared return type",
                f.name
            )),
        });
    }

    env.pop_scope();
}

// ─── Block inference ──────────────────────────────────────────────────────────

pub(super) fn infer_block(block: &Block<'_>, return_ty: &Ty, ctx: &mut InferCtx, env: &mut Env) -> Ty {
    env.push_scope();

    for s in &block.stmts {
        // All stmts in Block::stmts have has_semicolon: true (invariant from PR #20).
        // Their value is discarded; we type-check for side-effects and bindings only.
        stmt::infer_stmt(s, return_ty, ctx, env);
    }

    let tail_ty = match &block.tail {
        Some(e) => expr::infer_expr(e, ctx, env),
        None => Ty::Unit,
    };

    ctx.record(block.span, tail_ty.clone());
    env.pop_scope();
    tail_ty
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Emit a `Deferred` diagnostic and return `Ty::Error`.
/// Inference continues — callers treat `Ty::Error` as a cascade-suppressing sentinel.
pub(super) fn defer(ctx: &mut InferCtx, feature: &'static str, span: Span) -> Ty {
    ctx.error(TypeError::Deferred { feature, span });
    Ty::Error
}

/// Check that `found` matches `expected`, suppressing cascades when either is `Ty::Error`.
/// `suggestion_fn` is called lazily only when an error is emitted (avoids allocation
/// for the common success path).
pub(super) fn check_types<F>(
    expected: &Ty,
    found: &Ty,
    span: Span,
    ctx: &mut InferCtx,
    suggestion_fn: F,
) where
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

/// Emit `FieldOwnNonCopy` if the struct type that owns `object` is non-Copy.
/// Returns true when an error was emitted so callers can short-circuit.
/// The `expected_ref` guard (call args that expect `&T`) must be checked before
/// calling this — the helper only looks at struct Copy-ness, not parameter shape.
pub(super) fn check_field_own_non_copy(
    object: &Expr<'_>,
    field: &str,
    field_span: Span,
    field_ty: &Ty,
    ctx: &mut InferCtx,
) -> bool {
    if matches!(field_ty, Ty::Error) {
        return false;
    }
    let obj_ty = ctx.type_map.get(&object.span()).cloned().unwrap_or(Ty::Error);
    let struct_ty = match &obj_ty {
        Ty::Ref { inner, .. } => inner.as_ref().clone(),
        t => t.clone(),
    };
    // `Enum.Variant` parses as Expr::Field but is construction, not a field access —
    // no partial move can occur regardless of the enum's Copy-ness.
    if let Ty::Named(name) = &struct_ty {
        if ctx.enum_defs.contains_key(name.as_str()) {
            return false;
        }
    }
    if is_copy(&struct_ty, ctx) {
        return false;
    }
    ctx.error(TypeError::FieldOwnNonCopy {
        type_name: named_base_deref(&obj_ty),
        field_name: field.to_string(),
        span: field_span,
    });
    true
}

/// Checks FieldOwnNonCopy through transparent tail-position wrappers.
/// Recurses into block tails and if/else branches — any position where
/// the wrapped expression IS the value the surrounding expression produces.
/// Does not recurse into non-tail positions (conditions, binary operands, call args).
/// Precondition: `infer_expr` must have already run on `expr` (and all sub-expressions),
/// so `ctx.type_map` is populated for every `Expr::Field` span we encounter.
pub(super) fn check_tail_field_own_non_copy(expr: &Expr<'_>, ctx: &mut InferCtx) -> bool {
    match expr {
        Expr::Field { object, field, field_span, span } => {
            let field_ty = ctx.type_map.get(span).cloned().unwrap_or(Ty::Error);
            check_field_own_non_copy(object, field, *field_span, &field_ty, ctx)
        }
        Expr::Block(block) => match &block.tail {
            Some(e) => check_tail_field_own_non_copy(e, ctx),
            None => false,
        },
        Expr::If { then_block, else_branch, .. } => {
            let then_fired = match &then_block.tail {
                Some(e) => check_tail_field_own_non_copy(e, ctx),
                None => false,
            };
            let else_fired = match else_branch {
                Some(e) => check_tail_field_own_non_copy(e, ctx),
                None => false,
            };
            then_fired || else_fired
        }
        Expr::Match { arms, .. } => {
            arms.iter().any(|arm| check_tail_field_own_non_copy(&arm.body, ctx))
        }
        // Any other expression is not a transparent tail wrapper (Unary, Binary, Call, etc.).
        _ => false,
    }
}

/// Returns true if `ty` is Copy-eligible (§17 + §23).
/// Primitives and shared refs are always Copy. `&mut T` and non-Copy structs are not.
pub(super) fn is_copy(ty: &Ty, ctx: &InferCtx) -> bool {
    match ty {
        Ty::I32 | Ty::F64 | Ty::Bool | Ty::Char | Ty::Unit => true,
        Ty::Ref { mutable: false, .. } => true,
        Ty::Ref { mutable: true, .. } => false,
        Ty::Named(name) => {
            if let Some(info) = ctx.struct_defs.get(name.as_str()) {
                match info.copy_override {
                    Some(CopyMove::Copy) => true,
                    Some(CopyMove::Move) => false,
                    None => info.fields.values().all(|fty| is_copy(fty, ctx)),
                }
            } else if let Some(info) = ctx.enum_defs.get(name.as_str()) {
                match info.copy_override {
                    Some(CopyMove::Copy) => true,
                    Some(CopyMove::Move) => false,
                    // Enum is Copy iff all variant field types are Copy.
                    None => info.variants.values()
                        .flat_map(|fields| fields.iter())
                        .all(|fty| is_copy(fty, ctx)),
                }
            } else {
                false
            }
        }
        Ty::Str | Ty::Param(_) | Ty::TyVar(_) | Ty::Error => false,
    }
}

/// Extract the struct name from a type, auto-derefing one level of `Ty::Ref`.
/// Returns `"<unknown>"` when the type is not a named struct.
pub(super) fn named_base_deref(ty: &Ty) -> String {
    match ty {
        Ty::Named(n) => n.clone(),
        Ty::Ref { inner, .. } => match inner.as_ref() {
            Ty::Named(n) => n.clone(),
            _ => "<unknown>".to_string(),
        },
        _ => "<unknown>".to_string(),
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
        let Item::Function(f) = &ast.items[0] else { panic!("first item must be a function") };
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
    fn explicit_return_satisfies_declared_return_type() {
        // `fn main() -> i32 { return 42; }` — block has no tail, but the explicit
        // `return 42` satisfies the declared return type. Must not fire ReturnMismatch.
        assert!(check_fn_errors("fn main() -> i32 { return 42; }").is_empty());
        // Same for methods: `fn add(a: i32, b: i32) -> i32 { return a; }`
        assert!(check_fn_errors("fn add(a: i32, b: i32) -> i32 { return a; }").is_empty());
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
    fn deferred_field_access_on_primitive_non_fatal() {
        // i32 is not a struct — deferred with "field access on non-struct type", still non-fatal.
        let tokens = Lexer::new("fn f(n: i32) { let _x = n.foo; }").lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }

    // ── Method call resolution ────────────────────────────────────────────────

    fn infer_impl(src: &str) -> Result<crate::typechecker::InferResult, Vec<TypeError>> {
        let tokens = Lexer::new(src).lex().expect("lex failed in test");
        let ast = Parser::new(tokens).parse().expect("parse failed in test");
        typechecker::infer(&ast)
    }

    fn infer_impl_errors(src: &str) -> Vec<TypeError> {
        match infer_impl(src) {
            Ok(_) => vec![],
            Err(e) => e,
        }
    }

    #[test]
    fn error_method_not_found_on_primitive() {
        // Previously `deferred_method_call_non_fatal`. Now that method calls are
        // resolved, calling `.abs()` on `i32` (which has no impl block) is a fatal error.
        let errs = infer_impl_errors("fn f(n: i32) { n.abs(); }");
        assert!(errs.iter().any(|e| matches!(e, TypeError::MethodNotFound { .. })));
    }

    #[test]
    fn ok_method_call_returns_type() {
        // self.get() inside call_get should resolve to i32; no ReturnMismatch.
        let src = "impl Foo { \
            fn get(self) -> i32 { 0 } \
            fn call_get(self) -> i32 { self.get() } \
        }";
        assert!(infer_impl(src).is_ok());
    }

    #[test]
    fn error_method_not_found_wrong_name() {
        // `other` exists but `missing` does not — suggestion should name `other`.
        let src = "impl Foo { fn other(self) {} fn bad(self) { self.missing(); } }";
        let errs = infer_impl_errors(src);
        assert!(errs.iter().any(|e| {
            if let TypeError::MethodNotFound { method_name, suggestion, .. } = e {
                method_name == "missing"
                    && suggestion.as_deref().unwrap_or("").contains("other")
            } else {
                false
            }
        }));
    }

    #[test]
    fn error_method_arg_count_mismatch() {
        let src = "impl Foo { \
            fn add(self, x: i32) -> i32 { x } \
            fn test(self) { self.add(); } \
        }";
        let errs = infer_impl_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::ArgCountMismatch { name, expected: 1, found: 0, .. }
            if name == "Foo::add"
        )));
    }

    #[test]
    fn ok_move_self_binds_by_value() {
        // `move self` → Ty::Named("Foo") by value; no fatal errors.
        assert!(infer_impl("impl Foo { fn consume(move self) {} }").is_ok());
    }

    #[test]
    fn error_self_access_ambiguity() {
        // `take(self)` consumes self; `self.peek()` does not — ambiguity.
        let src = "\
            fn take(x: Foo) {} \
            impl Foo { \
                fn peek(self) {} \
                fn bad(self) { take(self); self.peek(); } \
            }";
        let errs = infer_impl_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::SelfAccessAmbiguity { fn_name, .. } if fn_name == "bad"
        )));
    }

    #[test]
    fn ok_method_cascade_suppression() {
        // Receiver type is Ty::Error (undefined var); no second error from method lookup.
        let src = "fn test() { missing.method(); }";
        let errs = infer_impl_errors(src);
        // Exactly one error: UndefinedVariable for `missing`; no MethodNotFound piled on.
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], TypeError::UndefinedVariable { name, .. } if name == "missing"));
    }

    #[test]
    fn ok_self_return_type_resolves() {
        // `Self` in return position resolves to Ty::Named("Foo"); no Deferred for Self.
        // `self` as tail → consuming → self: Ty::Named("Foo"); return type Ty::Named("Foo") matches.
        let src = "impl Foo { fn dup(self) -> Self { self } }";
        assert!(infer_impl(src).is_ok());
    }

    #[test]
    fn ok_self_ref_receiver_dispatch() {
        // Non-consuming self is bound as Ty::Ref { inner: Ty::Named("Foo") }.
        // Auto-deref in dispatch_type_name strips the Ref to find "Foo" in impl_sigs.
        let src = "impl Foo { \
            fn get(self) -> i32 { 0 } \
            fn call_get(self) -> i32 { self.get() } \
        }";
        assert!(infer_impl(src).is_ok());
    }

    #[test]
    fn error_method_arg_type_mismatch() {
        // Right arg count, wrong type: `add` expects i32, caller passes bool.
        let src = "impl Foo { \
            fn add(self, x: i32) -> i32 { x } \
            fn test(self) { self.add(true); } \
        }";
        let errs = infer_impl_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::Mismatch { expected: Ty::I32, found: Ty::Bool, .. }
        )));
    }

    #[test]
    fn error_self_mutating_and_consuming_ambiguity() {
        // `&mut self` in the body is a mutating (borrowing) use.
        // `take(self)` is a consuming use in the same body.
        // §18 widened predicate: mutating+consuming → SelfAccessAmbiguity, not silent by-value.
        let src = "\
            fn take(x: Foo) {} \
            impl Foo { \
                fn bad(self) { let _b = &mut self; take(self); } \
            }";
        let errs = infer_impl_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::SelfAccessAmbiguity { fn_name, .. } if fn_name == "bad"
        )));
    }

    #[test]
    fn error_consume_via_ref() {
        // `consume` declares `move self` (requires ownership).
        // `caller` has bare `self` → scanned as non-consuming → self bound as &Foo.
        // Calling `self.consume()` through &Foo must be a hard ConsumeViaRef error.
        let src = "impl Foo { \
            fn consume(move self) {} \
            fn caller(self) { self.consume(); } \
        }";
        let errs = infer_impl_errors(src);
        // Exactly one error — ConsumeViaRef does not cascade further.
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0],
            TypeError::ConsumeViaRef { type_name, method_name, .. }
            if type_name == "Foo" && method_name == "consume"
        ));
    }

    // ── Duplicate detection ───────────────────────────────────────────────────

    #[test]
    fn error_duplicate_free_fn() {
        let errs = check_fn_errors("fn foo() {} fn foo() {}");
        assert!(errs.iter().any(|e| matches!(e, TypeError::DuplicateFn { name, .. } if name == "foo")));
    }

    #[test]
    fn error_duplicate_method_same_type() {
        let src = "impl Foo { fn bar(self) {} } impl Foo { fn bar(self) {} }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        let errs = match typechecker::infer(&ast) {
            Ok(_) => vec![],
            Err(e) => e,
        };
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::DuplicateMethod { type_name, method_name, .. }
            if type_name == "Foo" && method_name == "bar"
        )));
    }

    #[test]
    fn ok_two_impl_blocks_non_overlapping() {
        let src = "impl Foo { fn a(self) {} } impl Foo { fn b(self) {} }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn ok_duplicate_method_name_different_types() {
        let src = "impl Foo { fn draw(self) {} } impl Bar { fn draw(self) {} }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn ok_free_fn_and_method_same_name() {
        let src = "fn draw() {} impl Foo { fn draw(self) {} }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn error_duplicate_fn_and_method_coexist() {
        let src = "fn foo() {} fn foo() {} impl Foo { fn bar() {} } impl Foo { fn bar() {} }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        let errs = match typechecker::infer(&ast) {
            Ok(_) => vec![],
            Err(e) => e,
        };
        assert!(errs.iter().any(|e| matches!(e, TypeError::DuplicateFn { .. })));
        assert!(errs.iter().any(|e| matches!(e, TypeError::DuplicateMethod { .. })));
    }

    // ── Struct field access (§23) ─────────────────────────────────────────────

    fn infer_program(src: &str) -> Result<crate::typechecker::InferResult, Vec<TypeError>> {
        let tokens = Lexer::new(src).lex().expect("lex failed");
        let ast = Parser::new(tokens).parse().expect("parse failed");
        typechecker::infer(&ast)
    }

    fn infer_program_errors(src: &str) -> Vec<TypeError> {
        match infer_program(src) {
            Ok(_) => vec![],
            Err(e) => e,
        }
    }

    #[test]
    fn ok_copy_field_read() {
        // f64 is Copy — reading p.x in let is fine.
        let src = "struct Point { x: f64, y: f64 } fn f(p: Point) -> f64 { let v = p.x; v }";
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn ok_borrow_of_non_copy_field() {
        // &entity.sprite is a borrow (Unary), not a direct Let/Return/call-arg move.
        // FieldOwnNonCopy only fires at those positions; borrowing a field is always fine.
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn f(e: Entity) -> &Sprite { &e.sprite }";
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn error_field_own_non_copy_let() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn f(e: Entity) { let _s = e.sprite; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite")));
    }

    #[test]
    fn error_field_own_non_copy_return() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn f(e: Entity) -> Sprite { return e.sprite; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite")));
    }

    #[test]
    fn error_field_own_non_copy_call_arg() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn consume(s: Sprite) {} \
                   fn f(e: Entity) { consume(e.sprite); }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite")));
    }

    #[test]
    fn error_field_write_via_shared_ref() {
        // Writing through &Point is forbidden.
        let src = "struct Point { x: f64 } fn f(r: &Point) { r.x = 1.0; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldWriteViaSharedRef { type_name, field_name, .. }
            if type_name == "Point" && field_name == "x"
        )));
    }

    #[test]
    fn error_field_not_found() {
        let src = "struct Point { x: f64, y: f64 } fn f(p: Point) -> f64 { p.z }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| {
            if let TypeError::FieldNotFound { field_name, available, .. } = e {
                field_name == "z" && available.contains(&"x".to_string())
            } else { false }
        }));
    }

    #[test]
    fn ok_cascade_suppression_on_error_receiver() {
        // Receiver is Ty::Error (undefined var) — no FieldNotFound piled on.
        let src = "struct Point { x: f64 } fn f() -> f64 { missing.x }";
        let errs = infer_program_errors(src);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], TypeError::UndefinedVariable { name, .. } if name == "missing"));
    }

    #[test]
    fn ok_copy_struct_override() {
        // `copy struct` → always Copy even if we explicitly use fields in let.
        let src = "copy struct Handle { fd: i32 } fn f(h: Handle) -> i32 { let v = h.fd; v }";
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn error_move_struct_override_non_copy_field() {
        // `move struct` with an i32 field: the struct is Move despite i32 being Copy.
        let src = "move struct Fd { raw: i32 } fn f(s: Fd) { let _x = s.raw; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "raw")));
    }

    #[test]
    fn ok_mutable_ref_field_write() {
        // Writing through &mut Point is valid — no FieldWriteViaSharedRef.
        let src = "struct Point { x: f64 } fn f(r: &mut Point) { r.x = 1.0; }";
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn deferred_generic_struct_field_access() {
        // Generic struct: field access is deferred (non-fatal) in phase 1.
        let src = "struct Cache<T> { val: T } fn f(c: Cache) -> i32 { c.val }";
        let tokens = Lexer::new(src).lex().expect("lex");
        let ast = Parser::new(tokens).parse().expect("parse");
        assert!(typechecker::infer(&ast).is_ok());
    }

    #[test]
    fn error_field_own_non_copy_through_shared_ref_receiver() {
        // `ref_entity: &Entity`, Entity is non-Copy because Sprite is move struct.
        // `ref_entity.sprite` is a direct Expr::Field — check fires despite receiver being a ref.
        // auto-deref in check_field_own_non_copy: obj_ty = &Entity → struct_ty = Entity → non-Copy.
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn f(r: &Entity) { let _s: Sprite = r.sprite; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    // ── check_tail_field_own_non_copy: block/if tail wrappers ────────────────

    #[test]
    fn error_field_own_non_copy_block_tail_let() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn f(e: Entity) { let _x: Sprite = { e.sprite }; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_block_tail_return() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn f(e: Entity) -> Sprite { return { e.sprite }; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_block_tail_call_arg() {
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn consume(_s: Sprite) {}                    fn f(e: Entity) { consume({ e.sprite }); }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_if_else_branches() {
        // let-init is Expr::If — both branches are field accesses on non-Copy struct.
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn f(e1: Entity, e2: Entity, cond: bool) {                        let _s = if cond { e1.sprite } else { e2.sprite };                    }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_implicit_return_bare() {
        // Function body tail is a bare Expr::Field — implicit return, no `return` keyword.
        // infer_fn must call check_tail_field_own_non_copy on f.body.tail (§23).
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn f(e: Entity) -> Sprite { e.sprite }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_implicit_return_if_else() {
        // Function body tail is Expr::If with field accesses in both branches.
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   fn f(e1: Entity, e2: Entity, cond: bool) -> Sprite { \
                       if cond { e1.sprite } else { e2.sprite } \
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn error_field_own_non_copy_match_arm_tail() {
        // Match arm tail is a non-Copy field access — check_tail_field_own_non_copy must
        // recurse into arm bodies (§23 + the NB comment resolved in this PR).
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite } \
                   enum Pick { First, Second } \
                   fn f(e1: Entity, e2: Entity, p: Pick) -> Sprite { \
                       match p { First => e1.sprite, Second => e2.sprite, } \
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::FieldOwnNonCopy { field_name, .. } if field_name == "sprite"
        )));
    }

    #[test]
    fn ok_field_copy_through_block_tail() {
        // Point is inferred-Copy (all fields f64) — block tail should not fire FieldOwnNonCopy.
        let src = "struct Point { x: f64 } fn f(p: Point) { let _x: f64 = { p.x }; }";
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn ok_field_borrow_in_block_tail() {
        // Block tail is &e.sprite (Expr::Unary { Ref, Expr::Field }) — not Expr::Field at tail.
        // helper hits _ => false → no FieldOwnNonCopy.
        let src = "move struct Sprite { id: i32 } struct Entity { sprite: Sprite }                    fn f(e: &Entity) { let _r = { &e.sprite }; }";
        let errs = infer_program_errors(src);
        assert!(!errs.iter().any(|e| matches!(e, TypeError::FieldOwnNonCopy { .. })));
    }

    #[test]
    fn error_duplicate_struct() {
        let src = "struct Foo { x: i32 } struct Foo { y: bool }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::DuplicateStruct { name, .. } if name == "Foo")));
    }

    // ── Struct literals ───────────────────────────────────────────────────────

    const POINT_DEF: &str = "struct Point { x: f64, y: f64 }";

    #[test]
    fn struct_lit_valid() {
        let src = &format!("{POINT_DEF} fn f() -> Point {{ Point {{ x = 1.0, y = 2.0 }} }}");
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn struct_lit_valid_any_field_order() {
        let src = &format!("{POINT_DEF} fn f() -> Point {{ Point {{ y = 2.0, x = 1.0 }} }}");
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn struct_lit_trailing_comma() {
        let src = &format!("{POINT_DEF} fn f() -> Point {{ Point {{ x = 1.0, y = 2.0, }} }}");
        assert!(infer_program(src).is_ok());
    }

    #[test]
    fn struct_lit_undefined_struct() {
        let src = "fn f() { let _ = Unknown { x = 1 }; }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::UndefinedStruct { name, .. } if name == "Unknown")));
    }

    #[test]
    fn struct_lit_unknown_field() {
        let src = &format!("{POINT_DEF} fn f() {{ let _ = Point {{ x = 1.0, z = 2.0 }}; }}");
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::FieldNotFound { field_name, .. } if field_name == "z")));
    }

    #[test]
    fn struct_lit_wrong_field_type() {
        let src = &format!("{POINT_DEF} fn f() {{ let _ = Point {{ x = true, y = 2.0 }}; }}");
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::Mismatch { expected: Ty::F64, .. })));
    }

    #[test]
    fn struct_lit_missing_field() {
        let src = &format!("{POINT_DEF} fn f() {{ let _ = Point {{ x = 1.0 }}; }}");
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::MissingStructFields { missing, .. } if missing.contains(&"y".to_string()))));
    }

    #[test]
    fn struct_lit_duplicate_field() {
        let src = &format!("{POINT_DEF} fn f() {{ let _ = Point {{ x = 1.0, x = 2.0, y = 0.0 }}; }}");
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::DuplicateStructField { field_name, .. } if field_name == "x")),
            "expected DuplicateStructField for x, got: {:?}", errs);
    }

    #[test]
    fn struct_lit_as_call_arg() {
        let src = &format!("{POINT_DEF} fn consume(_p: Point) {{}} fn f() {{ consume(Point {{ x = 1.0, y = 2.0 }}); }}");
        assert!(infer_program_errors(src).is_empty());
    }

    #[test]
    fn struct_lit_as_return_value() {
        let src = &format!("{POINT_DEF} fn f() -> Point {{ return Point {{ x = 0.0, y = 0.0 }}; }}");
        assert!(infer_program_errors(src).is_empty());
    }

    #[test]
    fn struct_lit_deferred_for_generic_struct() {
        let src = "struct Wrap<T> { val: T } fn f() { let _ = Wrap { val = 1 }; }";
        let result = infer_program(src).expect("no fatal errors");
        assert!(result.deferred.iter().any(|e| matches!(e, TypeError::Deferred { feature, .. } if feature.contains("generic struct"))));
    }

    // ── Bug 1 regression: return_ty propagation into if/while/loop blocks ────

    #[test]
    fn return_inside_if_then() {
        // Bug 1: `return 42` inside an if-then body saw Ty::Unit as the expected
        // return type and fired a false ReturnMismatch("expected Unit, found I32").
        assert!(check_fn_errors("fn f(b: bool) -> i32 { if b { return 42; } 0 }").is_empty());
    }

    #[test]
    fn return_inside_if_else() {
        // Same bug in the else branch.
        assert!(check_fn_errors(
            "fn f(b: bool) -> i32 { if b { let _ = 1; } else { return 99; } 0 }"
        ).is_empty());
    }

    #[test]
    fn return_inside_while_body() {
        assert!(check_fn_errors(
            "fn f() -> i32 { \
                let mut i = 0; \
                while i < 10 { if i == 5 { return i; } i = i + 1; } \
                0 \
            }"
        ).is_empty());
    }

    #[test]
    fn return_inside_loop_body() {
        assert!(check_fn_errors(
            "fn f() -> i32 { \
                let mut i = 0; \
                loop { i = i + 1; if i >= 5 { return i; } } \
                0 \
            }"
        ).is_empty());
    }

    #[test]
    fn return_inside_nested_else_if() {
        // Nested `else { if ... }` chain: both if-bodies have returns.
        assert!(check_fn_errors(
            "fn f(a: bool, b: bool) -> i32 { \
                if a { return 1; } else { if b { return 2; } } \
                0 \
            }"
        ).is_empty());
    }

    // ── Bug 2 regression: compound assignment type checking ───────────────────

    #[test]
    fn ok_compound_assign_i32() {
        // Bug 2: `x += 1` previously emitted Deferred and blocked codegen.
        assert!(check_fn_errors("fn f() -> i32 { let mut x = 0; x += 1; x }").is_empty());
    }

    #[test]
    fn error_compound_assign_bool_rhs() {
        // `x += true` must be a real Mismatch, not Deferred.
        // Proves Deferred was not silently hiding a genuine type-safety gap.
        let errs = check_fn_errors("fn f() -> i32 { let mut x = 0; x += true; x }");
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::Mismatch { expected: Ty::I32, found: Ty::Bool, .. }
        )), "expected Mismatch(I32, Bool), got: {:?}", errs);
    }

    #[test]
    fn ok_compound_assign_field() {
        // `p.x += 5`: compound assign on a struct field (exercises PR #35 codegen path).
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { let mut p = Point { x = 0, y = 0 }; p.x += 5; p.x }";
        assert!(infer_program_errors(src).is_empty());
    }

    #[test]
    fn error_compound_assign_field_via_shared_ref() {
        // Compound assign through a shared ref: FieldWriteViaSharedRef takes precedence;
        // check_binary_op_types is never reached, no secondary Mismatch piled on.
        let src = "struct Point { x: i32 } fn f(p: &Point) { p.x += 1; }";
        let errs = infer_program_errors(src);
        assert_eq!(errs.len(), 1, "expected exactly one error, got: {:?}", errs);
        assert!(matches!(&errs[0],
            TypeError::FieldWriteViaSharedRef { field_name, .. } if field_name == "x"
        ), "expected FieldWriteViaSharedRef, got: {:?}", errs[0]);
    }

    // ── Enum declarations (§20) ───────────────────────────────────────────────

    // T_en_01: bare unit variant resolves to the owning enum type.
    #[test]
    fn ok_bare_unit_variant() {
        let src = "enum Dir { North, South } fn f() -> Dir { North }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_02: bare tuple variant constructor resolves to the owning enum type.
    #[test]
    fn ok_bare_tuple_variant() {
        let src = "enum Shape { Circle(f64), Point } fn f() -> Shape { Circle(3.14) }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_03: qualified unit variant (`Dir.North`).
    #[test]
    fn ok_qualified_unit_variant() {
        let src = "enum Dir { North, South } fn f() -> Dir { Dir.North }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_04: qualified tuple variant (`Shape.Circle(3.14)`).
    #[test]
    fn ok_qualified_tuple_variant() {
        let src = "enum Shape { Circle(f64) } fn f() -> Shape { Shape.Circle(3.14) }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_05: `copy enum` declaration is accepted; no errors.
    #[test]
    fn ok_copy_enum_declaration() {
        let src = "copy enum Dir { North, South } fn f() -> Dir { North }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_06: `move enum` declaration is accepted; no errors.
    #[test]
    fn ok_move_enum_declaration() {
        let src = "move enum Handle { Open(i32), Closed } fn f() -> Handle { Closed }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_07: generic enum is accepted; variant access is deferred (non-fatal).
    #[test]
    fn ok_generic_enum_deferred() {
        let src = "enum Opt<T> { Some(T), None } fn f() {}";
        let result = infer_program(src).expect("no fatal errors");
        let _ = result;
    }

    // T_en_08: duplicate enum name is a fatal error.
    #[test]
    fn error_duplicate_enum() {
        let src = "enum Dir { North } enum Dir { South }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::DuplicateEnum { name, .. } if name == "Dir"
        )));
    }

    // T_en_09: duplicate variant name within the same enum is a fatal error.
    #[test]
    fn error_duplicate_variant_in_enum() {
        let src = "enum E { Foo, Bar, Foo } fn f() {}";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::DuplicateVariant { variant_name, .. } if variant_name == "Foo"
        )));
    }

    // T_en_10: wrong number of arguments to tuple variant constructor.
    #[test]
    fn error_variant_arg_count_mismatch() {
        let src = "enum Shape { Circle(f64) } fn f() -> Shape { Circle(1.0, 2.0) }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::VariantArgCountMismatch { variant_name, expected: 1, found: 2, .. }
            if variant_name == "Circle"
        )));
    }

    // T_en_11: unit variant called as a function.
    #[test]
    fn error_unit_variant_called_as_function() {
        let src = "enum Dir { North } fn f() -> Dir { North() }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::UnitVariantCalledAsFunction { variant_name, .. }
            if variant_name == "North"
        )));
    }

    // T_en_12: tuple variant used without arguments.
    #[test]
    fn error_tuple_variant_used_as_unit() {
        let src = "enum Shape { Circle(f64) } fn f() -> Shape { Circle }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::TupleVariantUsedAsUnit { variant_name, .. }
            if variant_name == "Circle"
        )));
    }

    // T_en_13a: two enums may declare the same variant name — valid at declaration time.
    #[test]
    fn ok_two_enums_same_variant_name_declaration() {
        let src = "enum A { Foo, Bar } enum B { Foo, Baz } fn f() {}";
        assert!(infer_program(src).is_ok());
    }

    // T_en_13b: qualified form always resolves correctly even when name is shared.
    #[test]
    fn ok_qualified_disambiguates_shared_variant_name() {
        let src = "enum A { Foo } enum B { Foo }                    fn use_a() -> A { A.Foo }                    fn use_b() -> B { B.Foo }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_16: `move enum` + qualified unit variant in tail — no FieldOwnNonCopy.
    #[test]
    fn ok_move_enum_qualified_unit_variant_tail() {
        let src = "move enum Dir { North, South } fn f() -> Dir { Dir.North }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_17: `move enum` + qualified unit variant in block tail — no FieldOwnNonCopy.
    #[test]
    fn ok_move_enum_qualified_unit_variant_block_tail() {
        let src = "move enum Dir { North, South } fn f() -> Dir { { Dir.North } }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_18: `move enum` + qualified unit variant in let — no FieldOwnNonCopy.
    #[test]
    fn ok_move_enum_qualified_unit_variant_let() {
        let src = "move enum Dir { North, South } fn f() -> Dir { let v = Dir.North; v }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_19: inferred-non-Copy enum (contains a move-struct field) + qualified variant.
    #[test]
    fn ok_non_copy_inferred_enum_qualified_variant() {
        // Handle wraps an i32 — but enum is non-Copy because str is non-Copy in one variant
        // Use a move enum instead since that's the reliable non-Copy path.
        let src = "move enum Handle { Open(i32), Closed } fn f() -> Handle { Handle.Closed }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_13c: bare form of a shared variant name is a fatal use-time error.
    #[test]
    fn error_ambiguous_bare_variant() {
        let src = "enum A { Foo } enum B { Foo } fn f() -> A { Foo }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::AmbiguousVariant { variant_name, defined_in, .. }
            if variant_name == "Foo" && defined_in.len() == 2
        )));
    }

    // T_en_13d: bare form of an unambiguous variant (shared name but DIFFERENT variant unambiguous).
    #[test]
    fn ok_unambiguous_bare_variant_with_sibling_ambiguous() {
        let src = "enum A { Foo, Bar } enum B { Foo, Baz }                    fn f() -> A { Bar }                    fn g() -> B { Baz }";
        assert!(infer_program(src).is_ok());
    }

    // T_en_14: variant argument type mismatch (tuple variant).
    #[test]
    fn error_variant_arg_type_mismatch() {
        let src = "enum Shape { Circle(f64) } fn f() -> Shape { Circle(42) }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::Mismatch { .. })));
    }

    // T_en_15: enum type used as a struct field type.
    #[test]
    fn ok_enum_as_struct_field_type() {
        let src = "enum Dir { North, South }                    struct Step { dir: Dir, dist: i32 }                    fn f() -> i32 {                        let s = Step { dir = North, dist = 5 };                        s.dist                    }";
        assert!(infer_program(src).is_ok());
    }

    // ── Match / pattern-matching tests (T_m_01 – T_m_19) ─────────────────────

    // T_m_01: exhaustive unit-variant enum match — all arms explicit, no wildcard.
    #[test]
    fn ok_match_enum_unit_variants_exhaustive() {
        let src = "enum Dir { North, South, East, West }
                   fn f(d: Dir) -> i32 {
                       match d { North => 0, South => 1, East => 2, West => 3, }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_02: exhaustive tuple-variant match — payload bound and used in body.
    #[test]
    fn ok_match_enum_tuple_variant_payload_binding() {
        let src = "enum Shape { Circle(f64), Rect(i32) }
                   fn f(s: Shape) -> i32 {
                       match s { Circle(r) => 0, Rect(w) => w, }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_03: non-exhaustive enum match — missing variant emits NonExhaustiveMatch.
    #[test]
    fn error_match_enum_non_exhaustive() {
        let src = "enum Dir { North, South, East }
                   fn f(d: Dir) -> i32 {
                       match d { North => 0, South => 1, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::NonExhaustiveMatch { missing, .. } if missing.contains(&"East".to_string())
        )));
    }

    // T_m_04: wildcard arm covers remaining variants — exhaustive.
    #[test]
    fn ok_match_enum_wildcard_covers_remainder() {
        let src = "enum Dir { North, South, East, West }
                   fn f(d: Dir) -> i32 {
                       match d { North => 0, _ => 1, }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_05: guarded arm does NOT count toward exhaustiveness — still non-exhaustive.
    #[test]
    fn error_match_guarded_arm_not_exhaustive() {
        let src = "enum Opt { Some(i32), None }
                   fn f(o: Opt) -> i32 {
                       match o { Some(x) if x > 0 => x, None => 0, }
                   }";
        let errs = infer_program_errors(src);
        // Some is covered only by a guarded arm, so it's missing from unguarded coverage.
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::NonExhaustiveMatch { missing, .. } if missing.contains(&"Some".to_string())
        )));
    }

    // T_m_06: bool match with explicit true + false — exhaustive without wildcard.
    #[test]
    fn ok_match_bool_explicit_true_false() {
        let src = "fn f(b: bool) -> i32 { match b { true => 1, false => 0, } }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_07: bool match missing false — non-exhaustive.
    #[test]
    fn error_match_bool_missing_false() {
        let src = "fn f(b: bool) -> i32 { match b { true => 1, } }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::NonExhaustiveMatch { missing, .. } if missing.contains(&"false".to_string())
        )));
    }

    // T_m_08: i32 subject without wildcard — non-exhaustive (open type requires _).
    #[test]
    fn error_match_i32_no_wildcard() {
        let src = "fn f(n: i32) -> i32 { match n { 0 => 1, 1 => 2, } }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::NonExhaustiveMatch { missing, .. } if missing.contains(&"_".to_string())
        )));
    }

    // T_m_09: i32 subject with wildcard — exhaustive.
    #[test]
    fn ok_match_i32_with_wildcard() {
        let src = "fn f(n: i32) -> i32 { match n { 0 => 1, _ => 2, } }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_10: payload binding typed correctly — Circle(r) binds r as f64.
    #[test]
    fn ok_match_payload_binding_typed() {
        let src = "enum Shape { Circle(f64) }
                   fn f(s: Shape) -> i32 {
                       match s { Circle(r) => 0, }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_11: payload count mismatch — Circle(r, s) where Circle(f64) has one field.
    #[test]
    fn error_match_pattern_arg_count_mismatch() {
        let src = "enum Shape { Circle(f64) }
                   fn f(s: Shape) -> i32 {
                       match s { Circle(r, extra) => 0, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::PatternArgCountMismatch { expected: 1, found: 2, .. }
        )));
    }

    // T_m_12: match is value-producing — result used as function return type.
    #[test]
    fn ok_match_value_producing() {
        let src = "enum Dir { North, South }
                   fn f(d: Dir) -> i32 {
                       let x = match d { North => 1, South => 2, };
                       x
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_13: arm type mismatch — one arm i32, another bool.
    #[test]
    fn error_match_arm_type_mismatch() {
        let src = "enum Dir { North, South }
                   fn f(d: Dir) -> i32 {
                       match d { North => 1, South => true, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::MatchArmMismatch { .. })));
    }

    // T_m_14: unreachable arm — explicit variant arm after unguarded wildcard.
    #[test]
    fn error_match_unreachable_arm_after_wildcard() {
        let src = "enum Dir { North, South }
                   fn f(d: Dir) -> i32 {
                       match d { _ => 0, North => 1, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::UnreachableArm { .. })));
    }

    // T_m_15: unreachable arm — explicit arm after unguarded binding.
    #[test]
    fn error_match_unreachable_arm_after_binding() {
        let src = "enum Dir { North, South }
                   fn f(d: Dir) -> i32 {
                       match d { x => 0, North => 1, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e, TypeError::UnreachableArm { .. })));
    }

    // T_m_16: qualified pattern in match arm — clear parse error, not "expected =>".
    #[test]
    fn error_match_qualified_pattern_rejected() {
        use crate::parser::parse_expr;
        let result = parse_expr("match s { Shape.Circle(r) => 0, _ => 1, }");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("qualified patterns") || msg.contains("bare variant"),
            "expected qualified-pattern message, got: {msg}");
    }

    // T_m_17: or-pattern counts both variants for exhaustiveness.
    #[test]
    fn ok_match_or_pattern_counts_both_variants() {
        let src = "enum Dir { North, South, East }
                   fn f(d: Dir) -> i32 {
                       match d { North | South => 0, East => 1, }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_18: guarded arm + separate unguarded arm for same variant — exhaustive.
    #[test]
    fn ok_match_guarded_plus_unguarded_exhaustive() {
        let src = "enum Opt { Some(i32), None }
                   fn f(o: Opt) -> i32 {
                       match o {
                           Some(x) if x > 0 => x,
                           Some(_) => 0,
                           None => -1,
                       }
                   }";
        assert!(infer_program(src).is_ok());
    }

    // T_m_19: unit variant used as constructor pattern — UnitVariantInConstructorPattern.
    #[test]
    fn error_match_unit_variant_as_constructor_pattern() {
        let src = "enum Dir { North, South }
                   fn f(d: Dir) -> i32 {
                       match d { North() => 0, South => 1, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::UnitVariantInConstructorPattern { variant_name, .. }
            if variant_name == "North"
        )));
    }

    // T_m_20: tuple variant used bare (no parentheses) in pattern — TupleVariantMissingPatternPayload,
    // with no cascading NonExhaustiveMatch for the same variant.
    #[test]
    fn error_match_tuple_variant_used_bare_in_pattern() {
        let src = "enum Shape { Circle(f64), Rect(i32) }
                   fn f(s: Shape) -> i32 {
                       match s { Circle => 0, Rect(w) => w, }
                   }";
        let errs = infer_program_errors(src);
        assert!(errs.iter().any(|e| matches!(e,
            TypeError::TupleVariantMissingPatternPayload { variant_name, .. }
            if variant_name == "Circle"
        )));
        // Must not also emit NonExhaustiveMatch for Circle — one error is enough.
        assert!(!errs.iter().any(|e| matches!(e,
            TypeError::NonExhaustiveMatch { missing, .. } if missing.contains(&"Circle".to_string())
        )));
    }

}
