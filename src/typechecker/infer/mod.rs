mod stmt;
mod expr;
mod ops;
mod convert;

use crate::ast::{Ast, Block, FunctionDef, Item, Type};
use crate::lexer::token::Span;
use crate::typechecker::env::{Env, InferCtx};
use crate::typechecker::error::TypeError;
use crate::typechecker::ty::{FnSig, Ty};
use crate::typechecker::InferResult;

// ─── Public entry point ───────────────────────────────────────────────────────

pub(crate) fn run(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    let mut ctx = InferCtx::new();

    // Pass 1: collect all function signatures before checking any body.
    // This allows mutual recursion and forward references.
    for item in &ast.items {
        match item {
            Item::Function(f) => collect_fn_sig(f, &mut ctx),
            Item::Impl(_) => {} // method type-checking deferred — future session
        }
    }

    // Pass 2: check each function body.
    let mut env = Env::new();
    for item in &ast.items {
        match item {
            Item::Function(f) => infer_fn(f, &mut ctx, &mut env),
            Item::Impl(_) => {} // method type-checking deferred — future session
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
        .map(|p| convert::ast_ty_to_ty(&p.ty, &f.generic_params, p.span, ctx))
        .collect();
    let return_ty = f
        .return_ty
        .as_ref()
        .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, f.span, ctx))
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
        .map(|t| convert::ast_ty_to_ty(t, &f.generic_params, f.span, ctx))
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

/// Bind a parameter name to its type. Handles `self` and `move self` receivers.
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
    // ⚠ METHOD/SELF CONTACT: both `self` and `move self` produce Type::SelfTy.
    // Defer until impl block design is in place (phase 2).
    if matches!(ty, Type::SelfTy(_)) {
        ctx.error(TypeError::Deferred {
            feature: "self receiver — requires impl block design",
            span,
        });
        return Ty::Error;
    }
    let _ = name;
    convert::ast_ty_to_ty(ty, generic_params, span, ctx)
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
