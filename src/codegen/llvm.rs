use inkwell::{
    FloatPredicate, IntPredicate, OptimizationLevel,
    builder::Builder,
    context::Context,
    module::Module,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicType, BasicTypeEnum},
    values::{BasicValueEnum, PointerValue},
};
use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Ast, BinOp, Expr, FunctionDef, Item, Literal, Stmt, Type, UnaryOp};
use crate::typechecker::{InferResult, Ty};

/// LLVM compilation context for one compiler invocation.
pub struct LlvmContext {
    inner: Context,
}

impl LlvmContext {
    pub fn new() -> Self {
        Self { inner: Context::create() }
    }

    // TODO: promote link_object/emit errors to a typed CodegenError enum (consistency with TypeError).

    /// Lower `ast` to a native binary at `out`.
    pub fn emit(&self, ast: &Ast<'_>, types: &InferResult, out: &Path) -> Result<(), String> {
        let module = lower_to_module(&self.inner, ast, types)?;
        emit_module(&module, out)
    }
}

impl Default for LlvmContext {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Module emission ──────────────────────────────────────────────────────────

fn emit_module(module: &Module<'_>, out: &Path) -> Result<(), String> {
    Target::initialize_x86(&InitializationConfig::default()); // x86-only for now; extend when multi-target lands

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).map_err(|e| e.to_string())?;
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            OptimizationLevel::None,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create target machine".to_string())?;

    let obj = out.with_extension("o");
    tm.write_to_file(module, FileType::Object, &obj)
        .map_err(|e| e.to_string())?;

    link_object(&obj, out)?;
    if let Err(e) = std::fs::remove_file(&obj) {
        eprintln!("ofan: warning: could not remove {}: {e}", obj.display());
    }
    Ok(())
}

fn link_object(obj: &Path, out: &Path) -> Result<(), String> {
    // Try each candidate in order; skip NotFound, continue past non-zero exits so
    // a broken `cc` shadowing a working `clang` doesn't block compilation.
    let mut last_error: Option<String> = None;
    for linker in linker_candidates() {
        match std::process::Command::new(&linker).arg(obj).arg("-o").arg(out).status() {
            Ok(s) if s.success() => return Ok(()),
            Ok(s) => last_error = Some(format!("{} exited with {s}", linker.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => last_error = Some(format!("failed to spawn {}: {e}", linker.display())),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        "no system linker found; install cc or clang and ensure it is in PATH".to_string()
    }))
}

fn linker_candidates() -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = vec!["cc".into(), "clang".into()];
    // Windows: also probe $LLVM_SYS_181_PREFIX\bin\clang.exe (set at build time
    // and still in env during development).
    if cfg!(windows) {
        if let Ok(prefix) = std::env::var("LLVM_SYS_181_PREFIX") {
            v.push(std::path::PathBuf::from(prefix).join("bin").join("clang.exe"));
        }
    }
    v
}

// ─── AST → LLVM IR lowering ───────────────────────────────────────────────────

type CodegenEnv<'ctx, 'src> = HashMap<&'src str, PointerValue<'ctx>>;

fn lower_to_module<'ctx>(
    ctx: &'ctx Context,
    ast: &Ast<'_>,
    types: &InferResult,
) -> Result<Module<'ctx>, String> {
    let module = ctx.create_module("main");
    for item in &ast.items {
        if let Item::Function(func) = item {
            lower_function(func, types, ctx, &module)?;
        }
        // Item::Struct / Item::Impl: out of scope for PR 31; PR 32+ will lower these.
    }
    Ok(module)
}

fn lower_function<'ctx, 'src>(
    func: &FunctionDef<'src>,
    types: &InferResult,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let fn_type = match func.return_ty.as_ref() {
        None => ctx.void_type().fn_type(&[], false),
        Some(ty) => basic_type_from_ast(ty, ctx)?.fn_type(&[], false),
    };
    let fn_val = module.add_function(func.name, fn_type, None);

    let entry = ctx.append_basic_block(fn_val, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry);

    let mut env: CodegenEnv<'ctx, 'src> = HashMap::new();

    // Pre-scan: emit all allocas at the top of the entry block before any
    // arithmetic instructions so mem2reg can promote them to registers.
    emit_allocas(&func.body.stmts, &builder, types, ctx, &mut env)?;

    for stmt in &func.body.stmts {
        lower_stmt(stmt, &builder, types, ctx, &mut env)?;
        if builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
        {
            break; // explicit `return` terminated the block; skip dead code
        }
    }

    // Tail return — only emitted when no explicit `return` in stmts.
    if builder
        .get_insert_block()
        .and_then(|b| b.get_terminator())
        .is_none()
    {
        if func.return_ty.is_none() {
            builder.build_return(None).map_err(|e| e.to_string())?;
        } else {
            match &func.body.tail {
                Some(tail) => {
                    let val = lower_expr(tail, &builder, types, ctx, &env)?;
                    builder
                        .build_return(Some(&val))
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    return Err(format!(
                        "function `{}`: body has no terminator and no tail expression",
                        func.name
                    ));
                }
            }
        }
    }

    Ok(())
}

fn emit_allocas<'ctx, 'src>(
    stmts: &[Stmt<'src>],
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    env: &mut CodegenEnv<'ctx, 'src>,
) -> Result<(), String> {
    for stmt in stmts {
        if let Stmt::Let { name, init, .. } = stmt {
            let ty = types
                .type_of(init.span())
                .ok_or_else(|| format!("missing type for let `{name}` initialiser"))?;
            let llvm_ty = basic_type(ty, ctx)?;
            let ptr = builder
                .build_alloca(llvm_ty, name)
                .map_err(|e| e.to_string())?;
            env.insert(*name, ptr);
        }
    }
    Ok(())
}

fn lower_stmt<'ctx, 'src>(
    stmt: &Stmt<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    env: &mut CodegenEnv<'ctx, 'src>,
) -> Result<(), String> {
    match stmt {
        Stmt::Let { name, init, .. } => {
            let val = lower_expr(init, builder, types, ctx, env)?;
            let ptr = *env.get(*name).ok_or_else(|| {
                format!("ICE: alloca for `{name}` missing — not pre-emitted by emit_allocas")
            })?;
            builder.build_store(ptr, val).map_err(|e| e.to_string())?;
        }
        Stmt::Return { value: Some(expr), .. } => {
            let val = lower_expr(expr, builder, types, ctx, env)?;
            builder
                .build_return(Some(&val))
                .map_err(|e| e.to_string())?;
        }
        Stmt::Return { value: None, .. } => {
            builder.build_return(None).map_err(|e| e.to_string())?;
        }
        Stmt::Expr { expr, .. } => {
            lower_expr(expr, builder, types, ctx, env)?;
        }
        _ => {
            return Err(
                "statement not supported in PR 31: only `let`, `return`, and expression \
                 statements are lowered; control flow and assignment land in PR 32"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn lower_expr<'ctx, 'src>(
    expr: &Expr<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    env: &CodegenEnv<'ctx, 'src>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match expr {
        Expr::Literal(lit, span) => {
            let ty = types
                .type_of(*span)
                .ok_or_else(|| "missing type for literal".to_string())?;
            match (lit, ty) {
                (Literal::Integer(n), Ty::I32) => {
                    if *n < i32::MIN as i64 || *n > i32::MAX as i64 {
                        return Err(format!(
                            "integer literal {n} out of range for i32 at byte {}: \
                             valid range {}..={} — annotate the wider type when it lands",
                            span.start, i32::MIN, i32::MAX
                        ));
                    }
                    Ok(ctx.i32_type().const_int(*n as u64, true).into())
                }
                (Literal::Float(f), Ty::F64) => {
                    Ok(ctx.f64_type().const_float(*f).into())
                }
                (Literal::Bool(b), Ty::Bool) => {
                    Ok(ctx.bool_type().const_int(*b as u64, false).into())
                }
                (lit, ty) => Err(format!(
                    "unsupported literal/type in PR 31: {lit:?} : {ty}"
                )),
            }
        }

        Expr::Ident(name, span) => {
            let ptr = *env.get(*name).ok_or_else(|| {
                format!("undefined variable in codegen: `{name}`")
            })?;
            let ty = types
                .type_of(*span)
                .ok_or_else(|| format!("missing type for identifier `{name}`"))?;
            let llvm_ty = basic_type(ty, ctx)?;
            builder
                .build_load(llvm_ty, ptr, name)
                .map_err(|e| e.to_string())
        }

        Expr::Binary { op, left, right, .. } => {
            let lv = lower_expr(left, builder, types, ctx, env)?;
            let rv = lower_expr(right, builder, types, ctx, env)?;
            let operand_ty = types
                .type_of(left.span())
                .ok_or_else(|| "missing type for binary left operand".to_string())?;
            lower_binary(*op, lv, rv, operand_ty, builder)
        }

        Expr::Unary { op, expr: inner, .. } => {
            let val = lower_expr(inner, builder, types, ctx, env)?;
            let ty = types
                .type_of(inner.span())
                .ok_or_else(|| "missing type for unary operand".to_string())?;
            match (op, ty) {
                (UnaryOp::Neg, Ty::I32) => builder
                    .build_int_neg(val.into_int_value(), "neg")
                    .map_err(|e| e.to_string())
                    .map(Into::into),
                (UnaryOp::Neg, Ty::F64) => builder
                    .build_float_neg(val.into_float_value(), "fneg")
                    .map_err(|e| e.to_string())
                    .map(Into::into),
                (UnaryOp::Not, Ty::Bool) => builder
                    .build_not(val.into_int_value(), "not")
                    .map_err(|e| e.to_string())
                    .map(Into::into),
                (op, ty) => Err(format!(
                    "unsupported unary op in PR 31: {op:?} on {ty}"
                )),
            }
        }

        _ => Err(format!(
            "expression not supported in PR 31 (at byte {}): only literals, \
             identifiers, and binary/unary operators are lowered; control flow \
             and function calls land in PR 32",
            expr.span().start
        )),
    }
}

fn lower_binary<'ctx>(
    op: BinOp,
    lv: BasicValueEnum<'ctx>,
    rv: BasicValueEnum<'ctx>,
    operand_ty: &Ty,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match operand_ty {
        Ty::I32 => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            Ok(match op {
                BinOp::Add => builder.build_int_add(l, r, "add").map_err(|e| e.to_string())?.into(),
                BinOp::Sub => builder.build_int_sub(l, r, "sub").map_err(|e| e.to_string())?.into(),
                BinOp::Mul => builder.build_int_mul(l, r, "mul").map_err(|e| e.to_string())?.into(),
                // PR 32 will add a runtime zero-divisor check (icmp + branch to abort).
                // Until then, LLVM's sdiv/srem exhibit hardware-trap behavior on div-by-zero
                // (SIGFPE on x86-64) — loud crash, not silent UB, but not a user-friendly error.
                BinOp::Div => builder.build_int_signed_div(l, r, "div").map_err(|e| e.to_string())?.into(),
                BinOp::Mod => builder.build_int_signed_rem(l, r, "rem").map_err(|e| e.to_string())?.into(),
                BinOp::Eq  => builder.build_int_compare(IntPredicate::EQ,  l, r, "eq" ).map_err(|e| e.to_string())?.into(),
                BinOp::Ne  => builder.build_int_compare(IntPredicate::NE,  l, r, "ne" ).map_err(|e| e.to_string())?.into(),
                BinOp::Lt  => builder.build_int_compare(IntPredicate::SLT, l, r, "lt" ).map_err(|e| e.to_string())?.into(),
                BinOp::Gt  => builder.build_int_compare(IntPredicate::SGT, l, r, "gt" ).map_err(|e| e.to_string())?.into(),
                BinOp::Le  => builder.build_int_compare(IntPredicate::SLE, l, r, "le" ).map_err(|e| e.to_string())?.into(),
                BinOp::Ge  => builder.build_int_compare(IntPredicate::SGE, l, r, "ge" ).map_err(|e| e.to_string())?.into(),
                _ => return Err(format!("operator {op:?} not supported for i32 in PR 31")),
            })
        }
        Ty::F64 => {
            let l = lv.into_float_value();
            let r = rv.into_float_value();
            Ok(match op {
                BinOp::Add => builder.build_float_add(l, r, "fadd").map_err(|e| e.to_string())?.into(),
                BinOp::Sub => builder.build_float_sub(l, r, "fsub").map_err(|e| e.to_string())?.into(),
                BinOp::Mul => builder.build_float_mul(l, r, "fmul").map_err(|e| e.to_string())?.into(),
                // Same deferred zero-divisor note as i32 above; f64 div-by-zero yields ±inf per IEEE 754.
                BinOp::Div => builder.build_float_div(l, r, "fdiv").map_err(|e| e.to_string())?.into(),
                BinOp::Mod => builder.build_float_rem(l, r, "frem").map_err(|e| e.to_string())?.into(),
                BinOp::Eq  => builder.build_float_compare(FloatPredicate::OEQ, l, r, "feq").map_err(|e| e.to_string())?.into(),
                BinOp::Ne  => builder.build_float_compare(FloatPredicate::ONE, l, r, "fne").map_err(|e| e.to_string())?.into(),
                BinOp::Lt  => builder.build_float_compare(FloatPredicate::OLT, l, r, "flt").map_err(|e| e.to_string())?.into(),
                BinOp::Gt  => builder.build_float_compare(FloatPredicate::OGT, l, r, "fgt").map_err(|e| e.to_string())?.into(),
                BinOp::Le  => builder.build_float_compare(FloatPredicate::OLE, l, r, "fle").map_err(|e| e.to_string())?.into(),
                BinOp::Ge  => builder.build_float_compare(FloatPredicate::OGE, l, r, "fge").map_err(|e| e.to_string())?.into(),
                _ => return Err(format!("operator {op:?} not supported for f64 in PR 31")),
            })
        }
        Ty::Bool => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            Ok(match op {
                BinOp::And => builder.build_and(l, r, "and").map_err(|e| e.to_string())?.into(),
                BinOp::Or  => builder.build_or(l, r, "or").map_err(|e| e.to_string())?.into(),
                _ => return Err(format!("operator {op:?} not supported for bool in PR 31")),
            })
        }
        ty => Err(format!(
            "binary op on unsupported operand type in PR 31: {ty}"
        )),
    }
}

// ─── Type helpers ─────────────────────────────────────────────────────────────

fn basic_type<'ctx>(ty: &Ty, ctx: &'ctx Context) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Ty::I32  => Ok(ctx.i32_type().into()),
        Ty::F64  => Ok(ctx.f64_type().into()),
        Ty::Bool => Ok(ctx.bool_type().into()),
        other => Err(format!("type not supported in PR 31 codegen: {other}")),
    }
}

fn basic_type_from_ast<'ctx>(ty: &Type<'_>, ctx: &'ctx Context) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Type::Named { name: "i32",  .. } => Ok(ctx.i32_type().into()),
        Type::Named { name: "f64",  .. } => Ok(ctx.f64_type().into()),
        Type::Named { name: "bool", .. } => Ok(ctx.bool_type().into()),
        other => Err(format!(
            "return type not supported in PR 31 codegen: {other:?}"
        )),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::typechecker;
    use inkwell::context::Context;

    fn compile_to_module<'ctx>(ctx: &'ctx Context, src: &str) -> Module<'ctx> {
        let tokens = Lexer::new(src).lex().expect("lex failed");
        let ast = Parser::new(tokens).parse().expect("parse failed");
        let types = typechecker::infer(&ast).expect("infer failed");
        lower_to_module(ctx, &ast, &types).expect("lower_to_module failed")
    }

    /// T4 — f64 arithmetic: 1.5 + 2.5 == 4.0 (JIT execution).
    #[test]
    fn test_f64_arithmetic_jit() {
        let ctx = Context::create();
        let src = "fn f64_add() -> f64 { 1.5 + 2.5 }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: f64 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> f64>("f64_add")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 4.0_f64);
    }

    /// T5 — comparison + let bindings: (3 < 4) == true (JIT execution).
    /// `bool` lowers to LLVM `i1`; the C ABI zero-extends `i1` to the low byte
    /// of rax on x86-64, so Rust's `u8` correctly reads 0 or 1.
    #[test]
    fn test_comparison_jit() {
        let ctx = Context::create();
        let src = "fn bool_cmp() -> bool { let a = 3; let b = 4; a < b }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: u8 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> u8>("bool_cmp")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 1u8, "expected 3 < 4 == true (1), got {result}");
    }
}
