use inkwell::{
    FloatPredicate, IntPredicate, OptimizationLevel,
    attributes::{Attribute, AttributeLoc},
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum},
    values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue},
};
use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Ast, BinOp, Block, Expr, FunctionDef, Item, Literal, Stmt, Type, UnaryOp};
use crate::typechecker::{InferResult, Ty};

/// LLVM compilation context for one compiler invocation.
pub struct LlvmContext {
    inner: Context,
}

impl LlvmContext {
    pub fn new() -> Self {
        Self { inner: Context::create() }
    }

    // TODO: promote errors to a typed CodegenError enum (PR 33+).

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
    // Windows: also probe $LLVM_SYS_181_PREFIX\bin\clang.exe (set at build time).
    if cfg!(windows) {
        if let Ok(prefix) = std::env::var("LLVM_SYS_181_PREFIX") {
            v.push(std::path::PathBuf::from(prefix).join("bin").join("clang.exe"));
        }
    }
    v
}

// ─── AST → LLVM IR lowering ───────────────────────────────────────────────────

/// Maps variable name → (alloca pointer, LLVM pointee type).
/// Storing the type avoids re-querying InferResult on every load and makes
/// compound assignment (+=, -=, …) straightforward.
type CodegenEnv<'ctx, 'src> = HashMap<&'src str, (PointerValue<'ctx>, BasicTypeEnum<'ctx>)>;

/// Loop exit/continue targets threaded through lower_stmt / lower_expr.
struct LoopCtx<'ctx> {
    break_bb: BasicBlock<'ctx>,
    continue_bb: BasicBlock<'ctx>,
}

/// Sentinel value used wherever Ofan's `()` appears in a value position.
/// The typechecker guarantees callers never use this result meaningfully.
fn unit_value<'ctx>(ctx: &'ctx Context) -> BasicValueEnum<'ctx> {
    ctx.bool_type().const_int(0, false).into()
}

fn lower_to_module<'ctx>(
    ctx: &'ctx Context,
    ast: &Ast<'_>,
    types: &InferResult,
) -> Result<Module<'ctx>, String> {
    let module = ctx.create_module("main");

    // Pass 1: declare all function signatures before lowering any body.
    // Required for forward calls (callee defined after caller) and recursive calls.
    for item in &ast.items {
        if let Item::Function(func) = item {
            declare_function_sig(func, ctx, &module)?;
        }
        // Item::Struct / Item::Impl: out of scope for PR 32.
    }

    // Pass 2: lower function bodies (all callees already visible in module).
    for item in &ast.items {
        if let Item::Function(func) = item {
            lower_function(func, types, ctx, &module)?;
        }
    }

    Ok(module)
}

/// Pass 1 helper: add the LLVM function declaration (signature only, no body).
fn declare_function_sig<'ctx>(
    func: &FunctionDef<'_>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    let param_types: Vec<BasicMetadataTypeEnum<'ctx>> = func
        .params
        .iter()
        .map(|p| basic_type_from_ast(&p.ty, ctx).map(Into::into))
        .collect::<Result<_, _>>()?;
    let fn_type = match func.return_ty.as_ref() {
        None => ctx.void_type().fn_type(&param_types, false),
        Some(ty) => basic_type_from_ast(ty, ctx)?.fn_type(&param_types, false),
    };
    module.add_function(func.name, fn_type, None);
    Ok(())
}

fn lower_function<'ctx, 'src>(
    func: &FunctionDef<'src>,
    types: &InferResult,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<(), String> {
    // Retrieve the pre-declared LLVM function from pass 1.
    let fn_val = module
        .get_function(func.name)
        .ok_or_else(|| format!("ICE: `{}` not pre-declared in pass 1", func.name))?;

    let entry = ctx.append_basic_block(fn_val, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry);

    let mut env: CodegenEnv<'ctx, 'src> = HashMap::new();

    // Phase 1: emit all allocas at the entry block top (canonical mem2reg form).
    // Params first, then top-level body lets.
    let mut param_alloca_entries: Vec<(&'src str, PointerValue<'ctx>, BasicTypeEnum<'ctx>)> =
        Vec::new();
    for param in &func.params {
        let llvm_ty = basic_type_from_ast(&param.ty, ctx)?;
        let ptr = builder
            .build_alloca(llvm_ty, param.name)
            .map_err(|e| e.to_string())?;
        param_alloca_entries.push((param.name, ptr, llvm_ty));
    }
    emit_allocas(&func.body.stmts, &builder, types, ctx, &mut env)?;

    // Phase 2: store param values into their allocas (after all alloca instructions).
    for (i, (name, ptr, llvm_ty)) in param_alloca_entries.into_iter().enumerate() {
        let param_val = fn_val
            .get_nth_param(i as u32)
            .ok_or_else(|| format!("ICE: missing param {i} for `{}`", func.name))?;
        builder.build_store(ptr, param_val).map_err(|e| e.to_string())?;
        env.insert(name, (ptr, llvm_ty));
    }

    // Phase 3: lower body statements.
    for stmt in &func.body.stmts {
        lower_stmt(stmt, &builder, types, ctx, module, fn_val, &mut env, None)?;
        if builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
        {
            break; // explicit return/break terminated the block; skip dead code
        }
    }

    // Phase 4: tail expression → return instruction (only when no explicit terminator).
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
                    let val =
                        lower_expr(tail, &builder, types, ctx, module, fn_val, &env, None)?;
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

/// Pre-scan `stmts` for `let` bindings and emit their allocas at the CURRENT builder
/// position.  Callers must ensure this is the function entry block for mem2reg eligibility.
/// Recurses into block-like `Stmt::Expr` so nested lets inside if/while/loop bodies are
/// also hoisted.  Shadowing (same name at multiple nesting depths) is deferred to PR 33:
/// when a name collision is detected the inner binding falls back to inline alloca emission
/// in `lower_stmt`.
fn emit_allocas<'ctx, 'src>(
    stmts: &[Stmt<'src>],
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    env: &mut CodegenEnv<'ctx, 'src>,
) -> Result<(), String> {
    for stmt in stmts {
        match stmt {
            Stmt::Let { name, init, .. } => {
                if env.contains_key(*name) {
                    // Known limitation (PR 33): shadowed bindings reuse the outer alloca.
                    // A full scope-stack is needed to assign each shadow its own slot.
                    continue;
                }
                let ty = types
                    .type_of(init.span())
                    .ok_or_else(|| format!("missing type for let `{name}` initialiser"))?;
                let llvm_ty = basic_type(ty, ctx)?;
                let ptr = builder
                    .build_alloca(llvm_ty, name)
                    .map_err(|e| e.to_string())?;
                env.insert(*name, (ptr, llvm_ty));
            }
            Stmt::Expr { expr, .. } => {
                emit_allocas_in_expr(expr, builder, types, ctx, env)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Recurse into block-like expressions to hoist their nested `let` allocas.
fn emit_allocas_in_expr<'ctx, 'src>(
    expr: &Expr<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    env: &mut CodegenEnv<'ctx, 'src>,
) -> Result<(), String> {
    match expr {
        Expr::If { then_block, else_branch, .. } => {
            emit_allocas(&then_block.stmts, builder, types, ctx, env)?;
            if let Some(tail) = &then_block.tail {
                emit_allocas_in_expr(tail, builder, types, ctx, env)?;
            }
            if let Some(else_expr) = else_branch {
                emit_allocas_in_expr(else_expr, builder, types, ctx, env)?;
            }
        }
        Expr::While { body, .. } | Expr::Loop { body, .. } => {
            emit_allocas(&body.stmts, builder, types, ctx, env)?;
            if let Some(tail) = &body.tail {
                emit_allocas_in_expr(tail, builder, types, ctx, env)?;
            }
        }
        Expr::Block(block) => {
            emit_allocas(&block.stmts, builder, types, ctx, env)?;
            if let Some(tail) = &block.tail {
                emit_allocas_in_expr(tail, builder, types, ctx, env)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Lower a braced block. Returns the tail expression value (if any); does NOT emit `ret`.
#[allow(clippy::too_many_arguments)]
fn lower_block<'ctx, 'src>(
    block: &Block<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    fn_val: FunctionValue<'ctx>,
    env: &mut CodegenEnv<'ctx, 'src>,
    loop_ctx: Option<&LoopCtx<'ctx>>,
) -> Result<Option<BasicValueEnum<'ctx>>, String> {
    for stmt in &block.stmts {
        lower_stmt(stmt, builder, types, ctx, module, fn_val, env, loop_ctx)?;
        if builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
        {
            return Ok(None); // break / continue / return terminated this path
        }
    }
    match &block.tail {
        Some(tail) => {
            lower_expr(tail, builder, types, ctx, module, fn_val, env, loop_ctx).map(Some)
        }
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_stmt<'ctx, 'src>(
    stmt: &Stmt<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    fn_val: FunctionValue<'ctx>,
    env: &mut CodegenEnv<'ctx, 'src>,
    loop_ctx: Option<&LoopCtx<'ctx>>,
) -> Result<(), String> {
    match stmt {
        Stmt::Let { name, init, .. } => {
            let val = lower_expr(init, builder, types, ctx, module, fn_val, env, loop_ctx)?;
            // Use the pre-hoisted alloca if available; otherwise emit inline (nested block).
            let (ptr, _) = if let Some(&entry) = env.get(*name) {
                entry
            } else {
                let ty = types
                    .type_of(init.span())
                    .ok_or_else(|| format!("missing type for let `{name}` initialiser"))?;
                let llvm_ty = basic_type(ty, ctx)?;
                let ptr = builder
                    .build_alloca(llvm_ty, name)
                    .map_err(|e| e.to_string())?;
                env.insert(*name, (ptr, llvm_ty));
                (ptr, llvm_ty)
            };
            builder.build_store(ptr, val).map_err(|e| e.to_string())?;
        }

        Stmt::Assign { target, op, value, .. } => {
            let Expr::Ident(name, _) = target.as_ref() else {
                return Err(
                    "assignment to non-identifier targets not supported in PR 32".to_string(),
                );
            };
            let &(ptr, llvm_ty) = env
                .get(*name)
                .ok_or_else(|| format!("undefined variable in assignment: `{name}`"))?;
            let rhs = lower_expr(value, builder, types, ctx, module, fn_val, env, loop_ctx)?;
            let new_val = match op {
                None => rhs,
                Some(binop) => {
                    let assign_ty = types.type_of(target.span()).ok_or_else(|| {
                        format!("missing type for assignment target `{name}`")
                    })?;
                    let current = builder
                        .build_load(llvm_ty, ptr, "load")
                        .map_err(|e| e.to_string())?;
                    lower_binary(*binop, current, rhs, assign_ty, builder, ctx, module, fn_val)?
                }
            };
            builder.build_store(ptr, new_val).map_err(|e| e.to_string())?;
        }

        Stmt::Return { value: Some(expr), .. } => {
            let val = lower_expr(expr, builder, types, ctx, module, fn_val, env, loop_ctx)?;
            builder
                .build_return(Some(&val))
                .map_err(|e| e.to_string())?;
        }
        Stmt::Return { value: None, .. } => {
            builder.build_return(None).map_err(|e| e.to_string())?;
        }

        Stmt::Break { value: Some(_), .. } => {
            return Err(
                "break with value not supported in PR 32 — assign to a variable before breaking"
                    .to_string(),
            );
        }
        Stmt::Break { value: None, .. } => {
            let lctx = loop_ctx.ok_or_else(|| {
                "ICE: break outside loop — should be caught by the typechecker".to_string()
            })?;
            builder
                .build_unconditional_branch(lctx.break_bb)
                .map_err(|e| e.to_string())?;
        }
        Stmt::Continue { .. } => {
            let lctx = loop_ctx.ok_or_else(|| {
                "ICE: continue outside loop — should be caught by the typechecker".to_string()
            })?;
            builder
                .build_unconditional_branch(lctx.continue_bb)
                .map_err(|e| e.to_string())?;
        }

        Stmt::Expr { expr, .. } => {
            lower_expr(expr, builder, types, ctx, module, fn_val, env, loop_ctx)?;
        }

        other => {
            return Err(format!(
                "statement not supported in PR 32 (at byte {}): \
                 `const` and other forms land in a later PR",
                stmt_span_start(other)
            ));
        }
    }
    Ok(())
}

fn stmt_span_start(stmt: &Stmt<'_>) -> usize {
    match stmt {
        Stmt::Let { span, .. }
        | Stmt::Const { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::Expr { span, .. } => span.start,
        Stmt::Continue { span } => span.start,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_expr<'ctx, 'src>(
    expr: &Expr<'src>,
    builder: &Builder<'ctx>,
    types: &InferResult,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    fn_val: FunctionValue<'ctx>,
    env: &CodegenEnv<'ctx, 'src>,
    loop_ctx: Option<&LoopCtx<'ctx>>,
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
                            span.start,
                            i32::MIN,
                            i32::MAX
                        ));
                    }
                    Ok(ctx.i32_type().const_int(*n as u64, true).into())
                }
                (Literal::Float(f), Ty::F64) => Ok(ctx.f64_type().const_float(*f).into()),
                (Literal::Bool(b), Ty::Bool) => {
                    Ok(ctx.bool_type().const_int(*b as u64, false).into())
                }
                (lit, ty) => Err(format!("unsupported literal/type: {lit:?} : {ty}")),
            }
        }

        Expr::Ident(name, _) => {
            let &(ptr, llvm_ty) = env
                .get(*name)
                .ok_or_else(|| format!("undefined variable in codegen: `{name}`"))?;
            builder
                .build_load(llvm_ty, ptr, name)
                .map_err(|e| e.to_string())
        }

        Expr::Binary { op, left, right, .. } => {
            let lv = lower_expr(left, builder, types, ctx, module, fn_val, env, loop_ctx)?;
            let rv = lower_expr(right, builder, types, ctx, module, fn_val, env, loop_ctx)?;
            let operand_ty = types
                .type_of(left.span())
                .ok_or_else(|| "missing type for binary left operand".to_string())?;
            lower_binary(*op, lv, rv, operand_ty, builder, ctx, module, fn_val)
        }

        Expr::Unary { op, expr: inner, .. } => {
            let val = lower_expr(inner, builder, types, ctx, module, fn_val, env, loop_ctx)?;
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
                (op, ty) => Err(format!("unsupported unary op: {op:?} on {ty}")),
            }
        }

        Expr::If { condition, then_block, else_branch, span } => {
            let cond_val = lower_expr(condition, builder, types, ctx, module, fn_val, env, loop_ctx)?
                .into_int_value();

            let if_ty = types.type_of(*span);
            let is_unit = matches!(if_ty, None | Some(Ty::Unit));

            let then_bb = ctx.append_basic_block(fn_val, "if.then");
            let merge_bb = ctx.append_basic_block(fn_val, "if.merge");

            if let Some(else_expr) = else_branch {
                let else_bb = ctx.append_basic_block(fn_val, "if.else");
                builder
                    .build_conditional_branch(cond_val, then_bb, else_bb)
                    .map_err(|e| e.to_string())?;

                // then branch
                builder.position_at_end(then_bb);
                let mut then_env = env.clone();
                let then_tail = lower_block(
                    then_block,
                    builder,
                    types,
                    ctx,
                    module,
                    fn_val,
                    &mut then_env,
                    loop_ctx,
                )?;
                let then_exit_bb = builder.get_insert_block().unwrap();
                let then_flows = then_exit_bb.get_terminator().is_none();
                if then_flows {
                    builder
                        .build_unconditional_branch(merge_bb)
                        .map_err(|e| e.to_string())?;
                }

                // else branch
                builder.position_at_end(else_bb);
                let else_env = env.clone();
                let else_val = lower_expr(
                    else_expr,
                    builder,
                    types,
                    ctx,
                    module,
                    fn_val,
                    &else_env,
                    loop_ctx,
                )?;
                let else_exit_bb = builder.get_insert_block().unwrap();
                let else_flows = else_exit_bb.get_terminator().is_none();
                if else_flows {
                    builder
                        .build_unconditional_branch(merge_bb)
                        .map_err(|e| e.to_string())?;
                }

                builder.position_at_end(merge_bb);

                // Unit if or both arms terminate early → no phi needed.
                if is_unit || (!then_flows && !else_flows) {
                    return Ok(unit_value(ctx));
                }

                // Value-producing if/else: emit phi node.
                let llvm_ty = basic_type(if_ty.unwrap(), ctx)?;
                let phi = builder
                    .build_phi(llvm_ty, "if.val")
                    .map_err(|e| e.to_string())?;
                if then_flows {
                    let tv = then_tail.unwrap_or_else(|| unit_value(ctx));
                    phi.add_incoming(&[(&tv as &dyn BasicValue<'ctx>, then_exit_bb)]);
                }
                if else_flows {
                    phi.add_incoming(&[(&else_val as &dyn BasicValue<'ctx>, else_exit_bb)]);
                }
                Ok(phi.as_basic_value())
            } else {
                // No else branch → always Unit.
                builder
                    .build_conditional_branch(cond_val, then_bb, merge_bb)
                    .map_err(|e| e.to_string())?;
                builder.position_at_end(then_bb);
                let mut then_env = env.clone();
                lower_block(
                    then_block,
                    builder,
                    types,
                    ctx,
                    module,
                    fn_val,
                    &mut then_env,
                    loop_ctx,
                )?;
                if builder
                    .get_insert_block()
                    .and_then(|b| b.get_terminator())
                    .is_none()
                {
                    builder
                        .build_unconditional_branch(merge_bb)
                        .map_err(|e| e.to_string())?;
                }
                builder.position_at_end(merge_bb);
                Ok(unit_value(ctx))
            }
        }

        Expr::While { condition, body, .. } => {
            let header_bb = ctx.append_basic_block(fn_val, "while.cond");
            let body_bb = ctx.append_basic_block(fn_val, "while.body");
            let exit_bb = ctx.append_basic_block(fn_val, "while.exit");

            builder
                .build_unconditional_branch(header_bb)
                .map_err(|e| e.to_string())?;

            builder.position_at_end(header_bb);
            let cond_val =
                lower_expr(condition, builder, types, ctx, module, fn_val, env, loop_ctx)?
                    .into_int_value();
            builder
                .build_conditional_branch(cond_val, body_bb, exit_bb)
                .map_err(|e| e.to_string())?;

            builder.position_at_end(body_bb);
            let inner_lctx = LoopCtx { break_bb: exit_bb, continue_bb: header_bb };
            let mut body_env = env.clone();
            lower_block(body, builder, types, ctx, module, fn_val, &mut body_env, Some(&inner_lctx))?;
            if builder
                .get_insert_block()
                .and_then(|b| b.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(header_bb)
                    .map_err(|e| e.to_string())?;
            }

            builder.position_at_end(exit_bb);
            Ok(unit_value(ctx))
        }

        Expr::Loop { body, .. } => {
            let loop_bb = ctx.append_basic_block(fn_val, "loop.body");
            let exit_bb = ctx.append_basic_block(fn_val, "loop.exit");

            builder
                .build_unconditional_branch(loop_bb)
                .map_err(|e| e.to_string())?;
            builder.position_at_end(loop_bb);

            let inner_lctx = LoopCtx { break_bb: exit_bb, continue_bb: loop_bb };
            let mut body_env = env.clone();
            lower_block(body, builder, types, ctx, module, fn_val, &mut body_env, Some(&inner_lctx))?;
            if builder
                .get_insert_block()
                .and_then(|b| b.get_terminator())
                .is_none()
            {
                builder
                    .build_unconditional_branch(loop_bb)
                    .map_err(|e| e.to_string())?;
            }

            builder.position_at_end(exit_bb);
            Ok(unit_value(ctx))
        }

        Expr::Block(block) => {
            let mut block_env = env.clone();
            match lower_block(block, builder, types, ctx, module, fn_val, &mut block_env, loop_ctx)? {
                Some(val) => Ok(val),
                None => Ok(unit_value(ctx)),
            }
        }

        Expr::Call { callee, args, .. } => {
            let Expr::Ident(name, _) = callee.as_ref() else {
                return Err(
                    "only direct function calls supported in PR 32 (no closures or fn pointers)"
                        .to_string(),
                );
            };
            let callee_fn = module.get_function(name).ok_or_else(|| {
                format!("undefined function `{name}` — declare it before calling")
            })?;
            let arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = args
                .iter()
                .map(|a| {
                    lower_expr(a, builder, types, ctx, module, fn_val, env, loop_ctx)
                        .map(Into::into)
                })
                .collect::<Result<_, _>>()?;
            let call_site = builder
                .build_call(callee_fn, &arg_vals, "call")
                .map_err(|e| e.to_string())?;
            // Void calls return unit_value; value-returning calls return the result.
            Ok(call_site.try_as_basic_value().basic().unwrap_or_else(|| unit_value(ctx)))
        }

        _ => Err(format!(
            "expression not supported in PR 32 (at byte {}): \
             field access, method calls, match, for, cast, and borrow land in later PRs",
            expr.span().start
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_binary<'ctx>(
    op: BinOp,
    lv: BasicValueEnum<'ctx>,
    rv: BasicValueEnum<'ctx>,
    operand_ty: &Ty,
    builder: &Builder<'ctx>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    fn_val: FunctionValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    match operand_ty {
        Ty::I32 => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            Ok(match op {
                BinOp::Add => {
                    builder.build_int_add(l, r, "add").map_err(|e| e.to_string())?.into()
                }
                BinOp::Sub => {
                    builder.build_int_sub(l, r, "sub").map_err(|e| e.to_string())?.into()
                }
                BinOp::Mul => {
                    builder.build_int_mul(l, r, "mul").map_err(|e| e.to_string())?.into()
                }
                // Div/Mod: runtime zero-divisor check → calls libc abort() (pillar 1).
                BinOp::Div => emit_int_div_or_rem(l, r, false, builder, ctx, module, fn_val)?,
                BinOp::Mod => emit_int_div_or_rem(l, r, true, builder, ctx, module, fn_val)?,
                BinOp::Eq => builder
                    .build_int_compare(IntPredicate::EQ, l, r, "eq")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Ne => builder
                    .build_int_compare(IntPredicate::NE, l, r, "ne")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Lt => builder
                    .build_int_compare(IntPredicate::SLT, l, r, "lt")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Gt => builder
                    .build_int_compare(IntPredicate::SGT, l, r, "gt")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Le => builder
                    .build_int_compare(IntPredicate::SLE, l, r, "le")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Ge => builder
                    .build_int_compare(IntPredicate::SGE, l, r, "ge")
                    .map_err(|e| e.to_string())?
                    .into(),
                _ => return Err(format!("operator {op:?} not supported for i32")),
            })
        }
        Ty::F64 => {
            let l = lv.into_float_value();
            let r = rv.into_float_value();
            // f64 div/mod: IEEE 754 defines ÷0 as ±inf/NaN — not UB, no abort needed.
            Ok(match op {
                BinOp::Add => {
                    builder.build_float_add(l, r, "fadd").map_err(|e| e.to_string())?.into()
                }
                BinOp::Sub => {
                    builder.build_float_sub(l, r, "fsub").map_err(|e| e.to_string())?.into()
                }
                BinOp::Mul => {
                    builder.build_float_mul(l, r, "fmul").map_err(|e| e.to_string())?.into()
                }
                BinOp::Div => {
                    builder.build_float_div(l, r, "fdiv").map_err(|e| e.to_string())?.into()
                }
                BinOp::Mod => {
                    builder.build_float_rem(l, r, "frem").map_err(|e| e.to_string())?.into()
                }
                BinOp::Eq => builder
                    .build_float_compare(FloatPredicate::OEQ, l, r, "feq")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Ne => builder
                    .build_float_compare(FloatPredicate::ONE, l, r, "fne")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Lt => builder
                    .build_float_compare(FloatPredicate::OLT, l, r, "flt")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Gt => builder
                    .build_float_compare(FloatPredicate::OGT, l, r, "fgt")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Le => builder
                    .build_float_compare(FloatPredicate::OLE, l, r, "fle")
                    .map_err(|e| e.to_string())?
                    .into(),
                BinOp::Ge => builder
                    .build_float_compare(FloatPredicate::OGE, l, r, "fge")
                    .map_err(|e| e.to_string())?
                    .into(),
                _ => return Err(format!("operator {op:?} not supported for f64")),
            })
        }
        Ty::Bool => {
            let l = lv.into_int_value();
            let r = rv.into_int_value();
            Ok(match op {
                BinOp::And => {
                    builder.build_and(l, r, "and").map_err(|e| e.to_string())?.into()
                }
                BinOp::Or => {
                    builder.build_or(l, r, "or").map_err(|e| e.to_string())?.into()
                }
                _ => return Err(format!("operator {op:?} not supported for bool")),
            })
        }
        ty => Err(format!("binary op on unsupported type: {ty}")),
    }
}

/// Emit an i32 div or rem with a runtime zero-divisor check.
/// Zero divisor → calls libc `abort()` and marks the block unreachable.
/// Pillar 1: explicit runtime panic, never silent UB.
fn emit_int_div_or_rem<'ctx>(
    l: IntValue<'ctx>,
    r: IntValue<'ctx>,
    is_rem: bool,
    builder: &Builder<'ctx>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    fn_val: FunctionValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, String> {
    let abort_fn = get_or_declare_abort(module, ctx);
    // Guard 1: divide by zero.
    let zero = ctx.i32_type().const_zero();
    let is_zero = builder
        .build_int_compare(IntPredicate::EQ, r, zero, "divz")
        .map_err(|e| e.to_string())?;
    // Guard 2: INT_MIN / -1 is signed overflow → LLVM poison.
    // -1 as u64 gives the correct bit pattern for const_int on an i32 type.
    let neg_one = ctx.i32_type().const_int(u64::MAX, false);
    let int_min = ctx.i32_type().const_int(i32::MIN as u64, false);
    let r_is_neg_one = builder
        .build_int_compare(IntPredicate::EQ, r, neg_one, "neg1")
        .map_err(|e| e.to_string())?;
    let l_is_int_min = builder
        .build_int_compare(IntPredicate::EQ, l, int_min, "minval")
        .map_err(|e| e.to_string())?;
    let is_overflow = builder
        .build_and(r_is_neg_one, l_is_int_min, "overflow")
        .map_err(|e| e.to_string())?;
    let is_bad = builder
        .build_or(is_zero, is_overflow, "divbad")
        .map_err(|e| e.to_string())?;

    let abort_bb = ctx.append_basic_block(fn_val, "div.abort");
    let ok_bb = ctx.append_basic_block(fn_val, "div.ok");
    builder
        .build_conditional_branch(is_bad, abort_bb, ok_bb)
        .map_err(|e| e.to_string())?;

    builder.position_at_end(abort_bb);
    builder.build_call(abort_fn, &[], "").map_err(|e| e.to_string())?;
    builder.build_unreachable().map_err(|e| e.to_string())?;

    builder.position_at_end(ok_bb);
    if is_rem {
        Ok(builder
            .build_int_signed_rem(l, r, "rem")
            .map_err(|e| e.to_string())?
            .into())
    } else {
        Ok(builder
            .build_int_signed_div(l, r, "div")
            .map_err(|e| e.to_string())?
            .into())
    }
}

fn get_or_declare_abort<'ctx>(module: &Module<'ctx>, ctx: &'ctx Context) -> FunctionValue<'ctx> {
    if let Some(f) = module.get_function("abort") {
        return f;
    }
    let ty = ctx.void_type().fn_type(&[], false);
    let f = module.add_function("abort", ty, Some(Linkage::External));
    // Mark noreturn so LLVM knows code after the call is unreachable (not UB-shaped).
    let noreturn = ctx.create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0);
    f.add_attribute(AttributeLoc::Function, noreturn);
    f
}

// ─── Type helpers ─────────────────────────────────────────────────────────────

fn basic_type<'ctx>(ty: &Ty, ctx: &'ctx Context) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Ty::I32 => Ok(ctx.i32_type().into()),
        Ty::F64 => Ok(ctx.f64_type().into()),
        Ty::Bool => Ok(ctx.bool_type().into()),
        other => Err(format!("type not supported in PR 32 codegen: {other}")),
    }
}

fn basic_type_from_ast<'ctx>(ty: &Type<'_>, ctx: &'ctx Context) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Type::Named { name: "i32", .. } => Ok(ctx.i32_type().into()),
        Type::Named { name: "f64", .. } => Ok(ctx.f64_type().into()),
        Type::Named { name: "bool", .. } => Ok(ctx.bool_type().into()),
        other => Err(format!("return type not supported in PR 32 codegen: {other:?}")),
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

    /// T4 — f64 arithmetic: 1.5 + 2.5 == 4.0 (JIT).
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

    /// T5 — comparison + let bindings: (3 < 4) == true (JIT).
    /// `bool` lowers to LLVM `i1`; C ABI zero-extends `i1` to low byte of rax.
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

    /// T6 — function call with parameters: add(3, 5) == 8 (JIT).
    #[test]
    fn test_function_call_jit() {
        let ctx = Context::create();
        let src = "fn add(a: i32, b: i32) -> i32 { a + b } fn call_add() -> i32 { add(3, 5) }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("call_add")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 8, "expected add(3, 5) == 8, got {result}");
    }

    /// T7 — if/else as expression: pick() selects 10 branch (JIT).
    #[test]
    fn test_if_else_expr_jit() {
        let ctx = Context::create();
        let src = "fn pick() -> i32 { if 1 < 2 { 10 } else { 20 } }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("pick")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 10, "expected if(1<2) {{10}} else {{20}} == 10, got {result}");
    }

    /// T8 — while loop + assignment: countdown from 3 reaches 0 (JIT).
    #[test]
    fn test_while_loop_jit() {
        let ctx = Context::create();
        let src = "fn countdown() -> i32 { let mut n = 3; while n > 0 { n = n - 1; } n }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("countdown")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 0, "expected countdown() == 0, got {result}");
    }

    /// T9 — loop + break + if + assignment: increments to 5 then breaks (JIT).
    #[test]
    fn test_loop_break_jit() {
        let ctx = Context::create();
        let src = "fn loop_break() -> i32 { let mut x = 0; loop { x = x + 1; if x == 5 { break; } } x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("loop_break")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 5, "expected loop_break() == 5, got {result}");
    }

    /// T10 — zero-divisor check: IR for `10 / 0` contains a call to abort (pillar 1).
    #[test]
    fn test_div_zero_emits_abort() {
        let ctx = Context::create();
        let src = "fn divz() -> i32 { 10 / 0 }";
        let module = compile_to_module(&ctx, src);
        let ir = module.print_to_string().to_string();
        assert!(
            ir.contains("call void @abort"),
            "expected abort call in IR for division by zero literal, IR:\n{ir}"
        );
    }

    /// T11 — forward call + iterative factorial: fact(5) == 120.
    /// `fact5` is declared BEFORE `fact` in source — exercises two-pass function
    /// declaration (single-pass would fail: `fact` not yet visible when `fact5` is lowered).
    #[test]
    fn test_forward_call_factorial_jit() {
        let ctx = Context::create();
        let src = "fn fact5() -> i32 { fact(5) } \
                   fn fact(n: i32) -> i32 { \
                       let mut acc = 1; \
                       let mut i = 1; \
                       while i <= n { acc = acc * i; i = i + 1; } \
                       acc \
                   }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("fact5")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 120, "expected fact(5) == 120, got {result}");
    }
}
