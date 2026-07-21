mod stmt;
mod expr;
mod ops;
mod convert;
mod self_access;

use crate::ast::{Ast, Block, CopyMove, Expr, FunctionDef, ImplBlock, Item, Stmt, StructDef, Type};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx, StructInfo};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{FnSig, Ty};
use crate::typechecker::InferResult;

// ─── Public entry point ───────────────────────────────────────────────────────

pub(crate) fn run(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    let mut ctx = InferCtx::new();

    // Sub-pass 1a: register struct names so 1b can resolve forward references.
    for item in &ast.items {
        if let Item::Struct(def) = item {
            collect_struct_name(def, &mut ctx);
        }
    }

    // Sub-pass 1b: populate struct field types (all names now known from 1a).
    for item in &ast.items {
        if let Item::Struct(def) = item {
            collect_struct_fields(def, &mut ctx);
        }
    }

    // Sub-pass 1c: collect fn/impl signatures.
    for item in &ast.items {
        match item {
            Item::Function(f) => collect_fn_sig(f, &mut ctx),
            Item::Impl(block) => collect_impl_sigs(block, &mut ctx),
            Item::Struct(_) => {}
        }
    }

    // Pass 2: check each function/method body.
    let mut env = Env::new();
    for item in &ast.items {
        match item {
            Item::Function(f) => infer_fn(f, &mut ctx, &mut env),
            Item::Impl(block) => infer_impl_methods(block, &mut ctx, &mut env),
            Item::Struct(_) => {}
        }
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

    let body_ty = infer_block(&f.body, &declared_return, ctx, env);

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

    let body_ty = infer_block(&f.body, &declared_return, ctx, env);

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
        // Any other expression is not a transparent tail wrapper (Unary, Binary, Call, etc.).
        // NB: when Expr::Match leaves deferred status it must be added here — it is a
        // value-producing tail wrapper and the wildcard will silently skip it otherwise.
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
        Ty::Named(name) => match ctx.struct_defs.get(name.as_str()) {
            Some(info) => match info.copy_override {
                Some(CopyMove::Copy) => true,
                Some(CopyMove::Move) => false,
                None => info.fields.values().all(|fty| is_copy(fty, ctx)),
            },
            None => false,
        },
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
}
