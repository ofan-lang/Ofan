use inkwell::{
    FloatPredicate, IntPredicate, OptimizationLevel,
    AddressSpace,
    attributes::{Attribute, AttributeLoc},
    basic_block::BasicBlock,
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, StructType},
    values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue},
};
use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Ast, BinOp, Block, Expr, FunctionDef, Item, Literal, Stmt, StructFieldInit, Type, UnaryOp};
use crate::lexer::token::Span;
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

/// Invariant lowering context for one LLVM function body.
///
/// Holds the five parameters that were previously threaded through every
/// lowering call as separate arguments, eliminating the parameter-threading
/// problem and the `#[allow(clippy::too_many_arguments)]` suppressions it caused.
///
/// Lifetime parameters:
/// - `'ctx`: LLVM context lifetime — all inkwell types carry this.
/// - `'b`: borrow lifetime for `module` and `types` references, which live
///   for one compilation pass but not the entire LLVM context lifetime.
///
/// The source-text lifetime `'src` (string slices in `CodegenEnv`, AST nodes)
/// appears only in method signatures, not as a struct field.
struct FnLower<'ctx, 'b> {
    /// IR builder; owned and created fresh per function body.
    builder: Builder<'ctx>,
    ctx: &'ctx Context,
    module: &'b Module<'ctx>,
    types: &'b InferResult,
    /// The LLVM function currently being populated; `Copy`.
    fn_val: FunctionValue<'ctx>,
    /// LLVM named struct types, keyed by Ofan struct name. Built in Pass 0.
    struct_types: &'b HashMap<String, StructType<'ctx>>,
    /// One pre-hoisted alloca per `Stmt::Let`, keyed by the binding's `name_span`.
    /// Populated by `emit_allocas` before any `lower_stmt` call; never mutated after.
    alloca_slots: HashMap<Span, PointerValue<'ctx>>,
}

impl<'ctx, 'b> FnLower<'ctx, 'b> {
    // ── Alloca hoisting ───────────────────────────────────────────────────────

    /// Pre-scan `stmts` for `let` bindings and emit their allocas at the CURRENT
    /// builder position into `self.alloca_slots`.  Callers must ensure this is the
    /// function entry block for mem2reg eligibility.  Recurses into block-like
    /// `Stmt::Expr` so nested lets inside if/while/loop bodies are also hoisted.
    ///
    /// Each `Stmt::Let` gets its own alloca keyed by `name_span`, so shadowed
    /// bindings — whether in the same block or a nested block — each have a
    /// distinct slot.  `lower_stmt` updates `env[name]` incrementally, ensuring
    /// `Expr::Ident` reads always resolve to the most-recently-processed binding.
    fn emit_allocas<'src>(&mut self, stmts: &[Stmt<'src>]) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, name_span, init, .. } => {
                    let ty = self
                        .types
                        .type_of(init.span())
                        .ok_or_else(|| format!("missing type for let `{name}` initialiser"))?;
                    let llvm_ty = self.llvm_ty(ty)?;
                    let ptr = self
                        .builder
                        .build_alloca(llvm_ty, name)
                        .map_err(|e| e.to_string())?;
                    self.alloca_slots.insert(*name_span, ptr);
                    // Hoist lets that live inside block-like init expressions
                    // (e.g. `let y = { let x = 1; x }`).
                    self.emit_allocas_in_expr(init)?;
                }
                Stmt::Expr { expr, .. } => {
                    self.emit_allocas_in_expr(expr)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Recurse into block-like expressions to hoist their nested `let` allocas.
    fn emit_allocas_in_expr<'src>(&mut self, expr: &Expr<'src>) -> Result<(), String> {
        match expr {
            Expr::If { then_block, else_branch, .. } => {
                self.emit_allocas(&then_block.stmts)?;
                if let Some(tail) = &then_block.tail {
                    self.emit_allocas_in_expr(tail)?;
                }
                if let Some(else_expr) = else_branch {
                    self.emit_allocas_in_expr(else_expr)?;
                }
            }
            Expr::While { body, .. } | Expr::Loop { body, .. } => {
                self.emit_allocas(&body.stmts)?;
                if let Some(tail) = &body.tail {
                    self.emit_allocas_in_expr(tail)?;
                }
            }
            Expr::Block(block) => {
                self.emit_allocas(&block.stmts)?;
                if let Some(tail) = &block.tail {
                    self.emit_allocas_in_expr(tail)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ── Type helpers ──────────────────────────────────────────────────────────

    /// Resolve `ty` to an LLVM `BasicTypeEnum`, including named struct types.
    fn llvm_ty(&self, ty: &Ty) -> Result<BasicTypeEnum<'ctx>, String> {
        resolve_ty(ty, self.ctx, self.struct_types)
    }

    /// Extract the struct name from the type recorded for `span`.
    ///
    /// Handles both bare `Named` (value-position struct) and `Ref { inner: Named }`
    /// (reference-receiver — the typechecker wraps non-consuming method receivers in Ref).
    /// Both map to the same LLVM `%StructType*` pointer.
    fn struct_name_of(&self, span: crate::lexer::token::Span) -> Result<&str, String> {
        match self.types.type_of(span) {
            Some(Ty::Named(n)) => Ok(n.as_str()),
            Some(Ty::Ref { inner, .. }) => match inner.as_ref() {
                Ty::Named(n) => Ok(n.as_str()),
                other => Err(format!(
                    "ICE: Ref inner type is not Named at byte {}: {other:?}",
                    span.start
                )),
            },
            other => Err(format!(
                "ICE: expected Named or Ref<Named> type at byte {}, got {:?}",
                span.start, other
            )),
        }
    }

    // ── Pointer-producing receiver helpers ────────────────────────────────────

    /// Lower `expr` to a pointer suitable for use as a method self-receiver or
    /// field-assignment target.
    ///
    /// - `Expr::Ident` → the pre-allocated variable alloca (already a pointer).
    /// - `Expr::StructLit` → fresh alloca filled by `lower_struct_lit_into`, pointer returned.
    /// - `Expr::Field` → GEP into the parent struct without a load (field pointer).
    /// - Anything else → lower to value, spill to a fresh alloca, return that pointer.
    fn lower_as_ptr<'src>(
        &self,
        expr: &Expr<'src>,
        env: &CodegenEnv<'ctx, 'src>,
        loop_ctx: Option<&LoopCtx<'ctx>>,
    ) -> Result<PointerValue<'ctx>, String> {
        match expr {
            Expr::Ident(name, _) => {
                let &(ptr, _) = env
                    .get(*name)
                    .ok_or_else(|| format!("ICE: undefined variable `{name}` in lower_as_ptr"))?;
                Ok(ptr)
            }
            Expr::StructLit { name, fields, .. } => {
                let struct_ty = *self
                    .struct_types
                    .get(*name)
                    .ok_or_else(|| format!("ICE: unknown struct `{name}` in lower_as_ptr"))?;
                let ptr = self
                    .builder
                    .build_alloca(struct_ty, "recv_tmp")
                    .map_err(|e| e.to_string())?;
                self.lower_struct_lit_into(ptr, struct_ty, name, fields, env, loop_ctx)?;
                Ok(ptr)
            }
            Expr::Field { object, field, .. } => {
                let obj_ptr = self.lower_as_ptr(object, env, loop_ctx)?;
                let struct_name = self.struct_name_of(object.span())?;
                let struct_ty = *self.struct_types.get(struct_name).ok_or_else(|| {
                    format!("ICE: struct `{struct_name}` not in struct_types")
                })?;
                let idx = self
                    .types
                    .struct_field_index(struct_name, field)
                    .ok_or_else(|| {
                        format!("ICE: field `{field}` not found in struct `{struct_name}`")
                    })? as u32;
                self.builder
                    .build_struct_gep(struct_ty, obj_ptr, idx, "field_ptr")
                    .map_err(|e| e.to_string())
            }
            other => {
                let val = self.lower_expr(other, env, loop_ctx)?;
                // lower_expr returns a PointerValue (struct alloca) for Expr::StructLit and any
                // transparent wrapper (Block, If) around one. Return it directly — building a
                // new alloca and storing the pointer would produce a pointer-to-pointer.
                if let BasicValueEnum::PointerValue(ptr) = val {
                    if matches!(self.types.type_of(other.span()), Some(Ty::Named(_))) {
                        return Ok(ptr);
                    }
                }
                let ty = val.get_type();
                let ptr = self.builder.build_alloca(ty, "spill").map_err(|e| e.to_string())?;
                self.builder.build_store(ptr, val).map_err(|e| e.to_string())?;
                Ok(ptr)
            }
        }
    }

    /// Store struct literal fields into `dest` in declaration order.
    ///
    /// The literal fields may appear in any order; this always writes them in
    /// `field_order` (struct declaration order) to match the LLVM struct layout.
    fn lower_struct_lit_into<'src>(
        &self,
        dest: PointerValue<'ctx>,
        struct_ty: StructType<'ctx>,
        type_name: &str,
        fields: &[StructFieldInit<'src>],
        env: &CodegenEnv<'ctx, 'src>,
        loop_ctx: Option<&LoopCtx<'ctx>>,
    ) -> Result<(), String> {
        let field_names = self
            .types
            .struct_field_names(type_name)
            .ok_or_else(|| format!("ICE: struct `{type_name}` not in InferResult"))?;
        for (i, fname) in field_names.iter().enumerate() {
            let init_expr = fields
                .iter()
                .find(|f| f.name == fname)
                .ok_or_else(|| format!("ICE: missing field `{fname}` in struct literal"))?;
            let gep = self
                .builder
                .build_struct_gep(struct_ty, dest, i as u32, "field_init")
                .map_err(|e| e.to_string())?;
            // Mirrors Stmt::Let: fast-path StructLit directly into dest, then lower-once
            // for everything else and match on the (value, type) pair.
            match init_expr.value.as_ref() {
                Expr::StructLit { name: sname, fields: inner_fields, .. } => {
                    // Nested struct literal: write directly into the GEP slot — no temp alloca.
                    let inner_ty = *self.struct_types.get(*sname).ok_or_else(|| {
                        format!("ICE: struct `{sname}` not in struct_types")
                    })?;
                    self.lower_struct_lit_into(gep, inner_ty, sname, inner_fields, env, loop_ctx)?;
                }
                _ => {
                    let val = self.lower_expr(&init_expr.value, env, loop_ctx)?;
                    // Struct-typed inits that flow through a Block/If wrapper return a
                    // PointerValue (temp alloca). Load the struct value before storing.
                    match (val, self.types.type_of(init_expr.value.span())) {
                        (BasicValueEnum::PointerValue(src_ptr), Some(Ty::Named(sname))) => {
                            let inner_ty =
                                *self.struct_types.get(sname.as_str()).ok_or_else(|| {
                                    format!("ICE: struct `{sname}` not in struct_types")
                                })?;
                            let struct_val = self
                                .builder
                                .build_load(inner_ty, src_ptr, "field_struct")
                                .map_err(|e| e.to_string())?;
                            self.builder.build_store(gep, struct_val).map_err(|e| e.to_string())?;
                        }
                        (other_val, _) => {
                            self.builder.build_store(gep, other_val).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ── Block / statement lowering ────────────────────────────────────────────

    /// Lower a braced block. Returns the tail expression value (if any); does NOT emit `ret`.
    fn lower_block<'src>(
        &self,
        block: &Block<'src>,
        env: &mut CodegenEnv<'ctx, 'src>,
        loop_ctx: Option<&LoopCtx<'ctx>>,
    ) -> Result<Option<BasicValueEnum<'ctx>>, String> {
        for stmt in &block.stmts {
            self.lower_stmt(stmt, env, loop_ctx)?;
            if self
                .builder
                .get_insert_block()
                .and_then(|b| b.get_terminator())
                .is_some()
            {
                return Ok(None); // break / continue / return terminated this path
            }
        }
        match &block.tail {
            Some(tail) => self.lower_expr(tail, env, loop_ctx).map(Some),
            None => Ok(None),
        }
    }

    fn lower_stmt<'src>(
        &self,
        stmt: &Stmt<'src>,
        env: &mut CodegenEnv<'ctx, 'src>,
        loop_ctx: Option<&LoopCtx<'ctx>>,
    ) -> Result<(), String> {
        match stmt {
            Stmt::Let { name, name_span, init, .. } => {
                // Retrieve the pre-hoisted alloca for this specific binding (keyed by its
                // declaration span, so shadowed bindings each get their own slot).
                // Update env[name] so subsequent Expr::Ident reads find this alloca.
                let ty = self
                    .types
                    .type_of(init.span())
                    .ok_or_else(|| format!("missing type for let `{name}` initialiser"))?;
                let llvm_ty = self.llvm_ty(ty)?;
                // Use the pre-hoisted alloca when available (mem2reg-eligible). Fall back to
                // inline alloca for lets inside sub-expressions that emit_allocas_in_expr
                // doesn't descend into (e.g. block operands of Binary, Call, MethodCall).
                let ptr = if let Some(&p) = self.alloca_slots.get(name_span) {
                    p
                } else {
                    self.builder.build_alloca(llvm_ty, name).map_err(|e| e.to_string())?
                };
                env.insert(*name, (ptr, llvm_ty));
                // Struct literals are lowered field-by-field directly into the destination
                // alloca; other expressions are lowered to a value and stored normally.
                match init.as_ref() {
                    Expr::StructLit { name: sname, fields, .. } => {
                        let struct_ty = llvm_ty.into_struct_type();
                        self.lower_struct_lit_into(ptr, struct_ty, sname, fields, env, loop_ctx)?;
                    }
                    _ => {
                        let val = self.lower_expr(init, env, loop_ctx)?;
                        // Struct-typed inits flowing through a Block or If wrapper: lower_expr
                        // returns a PointerValue (temp struct alloca). Load the struct value
                        // from that pointer and store into the pre-hoisted dest alloca, rather
                        // than storing the pointer bits into the struct slot.
                        match (val, self.types.type_of(init.span())) {
                            (BasicValueEnum::PointerValue(src_ptr), Some(Ty::Named(sname))) => {
                                let struct_ty =
                                    *self.struct_types.get(sname.as_str()).ok_or_else(|| {
                                        format!("ICE: struct `{sname}` not in struct_types")
                                    })?;
                                let struct_val = self
                                    .builder
                                    .build_load(struct_ty, src_ptr, "struct_tmp")
                                    .map_err(|e| e.to_string())?;
                                self.builder
                                    .build_store(ptr, struct_val)
                                    .map_err(|e| e.to_string())?;
                            }
                            (other_val, _) => {
                                self.builder
                                    .build_store(ptr, other_val)
                                    .map_err(|e| e.to_string())?;
                            }
                        }
                    }
                }
            }

            Stmt::Assign { target, op, value, .. } => {
                match target.as_ref() {
                    Expr::Ident(name, _) => {
                        let &(ptr, llvm_ty) = env
                            .get(*name)
                            .ok_or_else(|| format!("undefined variable in assignment: `{name}`"))?;
                        let rhs = self.lower_expr(value, env, loop_ctx)?;
                        let new_val = match op {
                            None => rhs,
                            Some(binop) => {
                                let assign_ty =
                                    self.types.type_of(target.span()).ok_or_else(|| {
                                        format!("missing type for assignment target `{name}`")
                                    })?;
                                let current = self
                                    .builder
                                    .build_load(llvm_ty, ptr, "load")
                                    .map_err(|e| e.to_string())?;
                                self.lower_binary(*binop, current, rhs, assign_ty)?
                            }
                        };
                        self.builder.build_store(ptr, new_val).map_err(|e| e.to_string())?;
                    }
                    Expr::Field { object, field, .. } => {
                        let obj_ptr = self.lower_as_ptr(object, env, loop_ctx)?;
                        let struct_name = self.struct_name_of(object.span())?;
                        let struct_ty = *self.struct_types.get(struct_name).ok_or_else(|| {
                            format!("ICE: struct `{struct_name}` not in struct_types")
                        })?;
                        let idx = self
                            .types
                            .struct_field_index(struct_name, field)
                            .ok_or_else(|| {
                                format!(
                                    "ICE: field `{field}` not found in struct `{struct_name}`"
                                )
                            })? as u32;
                        let gep = self
                            .builder
                            .build_struct_gep(struct_ty, obj_ptr, idx, "field_ptr")
                            .map_err(|e| e.to_string())?;
                        let rhs = self.lower_expr(value, env, loop_ctx)?;
                        let new_val = match op {
                            None => rhs,
                            Some(binop) => {
                                let field_ty = self.llvm_ty(
                                    self.types.struct_field_type(struct_name, field).ok_or_else(
                                        || {
                                            format!("ICE: field `{field}` type not found in struct `{struct_name}`")
                                        },
                                    )?,
                                )?;
                                let current = self
                                    .builder
                                    .build_load(field_ty, gep, "field_cur")
                                    .map_err(|e| e.to_string())?;
                                let assign_ty = self
                                    .types
                                    .type_of(target.span())
                                    .ok_or_else(|| "missing type for field assign target".to_string())?;
                                self.lower_binary(*binop, current, rhs, assign_ty)?
                            }
                        };
                        self.builder.build_store(gep, new_val).map_err(|e| e.to_string())?;
                    }
                    _ => {
                        return Err(
                            "assignment to non-identifier/field targets not yet supported".to_string(),
                        )
                    }
                }
            }

            Stmt::Return { value: Some(expr), .. } => {
                let val = self.lower_expr(expr, env, loop_ctx)?;
                self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
            }
            Stmt::Return { value: None, .. } => {
                self.builder.build_return(None).map_err(|e| e.to_string())?;
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
                self.builder
                    .build_unconditional_branch(lctx.break_bb)
                    .map_err(|e| e.to_string())?;
            }
            Stmt::Continue { .. } => {
                let lctx = loop_ctx.ok_or_else(|| {
                    "ICE: continue outside loop — should be caught by the typechecker".to_string()
                })?;
                self.builder
                    .build_unconditional_branch(lctx.continue_bb)
                    .map_err(|e| e.to_string())?;
            }

            Stmt::Expr { expr, .. } => {
                self.lower_expr(expr, env, loop_ctx)?;
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

    // ── Expression lowering ───────────────────────────────────────────────────

    fn lower_expr<'src>(
        &self,
        expr: &Expr<'src>,
        env: &CodegenEnv<'ctx, 'src>,
        loop_ctx: Option<&LoopCtx<'ctx>>,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expr::Literal(lit, span) => {
                let ty = self
                    .types
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
                        Ok(self.ctx.i32_type().const_int(*n as u64, true).into())
                    }
                    (Literal::Float(f), Ty::F64) => {
                        Ok(self.ctx.f64_type().const_float(*f).into())
                    }
                    (Literal::Bool(b), Ty::Bool) => {
                        Ok(self.ctx.bool_type().const_int(*b as u64, false).into())
                    }
                    (lit, ty) => Err(format!("unsupported literal/type: {lit:?} : {ty}")),
                }
            }

            Expr::Ident(name, _) => {
                let &(ptr, llvm_ty) = env
                    .get(*name)
                    .ok_or_else(|| format!("undefined variable in codegen: `{name}`"))?;
                self.builder
                    .build_load(llvm_ty, ptr, name)
                    .map_err(|e| e.to_string())
            }

            Expr::Binary { op, left, right, .. } => {
                let lv = self.lower_expr(left, env, loop_ctx)?;
                let rv = self.lower_expr(right, env, loop_ctx)?;
                let operand_ty = self
                    .types
                    .type_of(left.span())
                    .ok_or_else(|| "missing type for binary left operand".to_string())?;
                self.lower_binary(*op, lv, rv, operand_ty)
            }

            Expr::Unary { op, expr: inner, .. } => {
                // Fold Neg(Literal::Integer(2147483648)) → i32::MIN constant directly.
                // The typechecker blesses this via the infer_unary INT_MIN special case,
                // but the literal lowering guard rejects 2147483648 > i32::MAX. Bypass it.
                if matches!(op, UnaryOp::Neg) {
                    if let Expr::Literal(Literal::Integer(n), _) = inner.as_ref() {
                        if *n == (i32::MAX as i64) + 1 {
                            return Ok(self.ctx.i32_type().const_int(i32::MIN as u64, false).into());
                        }
                    }
                }
                let val = self.lower_expr(inner, env, loop_ctx)?;
                let ty = self
                    .types
                    .type_of(inner.span())
                    .ok_or_else(|| "missing type for unary operand".to_string())?;
                match (op, ty) {
                    (UnaryOp::Neg, Ty::I32) => self
                        .builder
                        .build_int_neg(val.into_int_value(), "neg")
                        .map_err(|e| e.to_string())
                        .map(Into::into),
                    (UnaryOp::Neg, Ty::F64) => self
                        .builder
                        .build_float_neg(val.into_float_value(), "fneg")
                        .map_err(|e| e.to_string())
                        .map(Into::into),
                    (UnaryOp::Not, Ty::Bool) => self
                        .builder
                        .build_not(val.into_int_value(), "not")
                        .map_err(|e| e.to_string())
                        .map(Into::into),
                    (op, ty) => Err(format!("unsupported unary op: {op:?} on {ty}")),
                }
            }

            Expr::If { condition, then_block, else_branch, span } => {
                let cond_val = self
                    .lower_expr(condition, env, loop_ctx)?
                    .into_int_value();

                let if_ty = self.types.type_of(*span);
                let is_unit = matches!(if_ty, None | Some(Ty::Unit));

                let then_bb = self.ctx.append_basic_block(self.fn_val, "if.then");
                let merge_bb = self.ctx.append_basic_block(self.fn_val, "if.merge");

                if let Some(else_expr) = else_branch {
                    let else_bb = self.ctx.append_basic_block(self.fn_val, "if.else");
                    self.builder
                        .build_conditional_branch(cond_val, then_bb, else_bb)
                        .map_err(|e| e.to_string())?;

                    // then branch
                    self.builder.position_at_end(then_bb);
                    let mut then_env = env.clone();
                    let then_tail =
                        self.lower_block(then_block, &mut then_env, loop_ctx)?;
                    let then_exit_bb = self.builder.get_insert_block().unwrap();
                    let then_flows = then_exit_bb.get_terminator().is_none();
                    if then_flows {
                        self.builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(|e| e.to_string())?;
                    }

                    // else branch
                    self.builder.position_at_end(else_bb);
                    let else_env = env.clone();
                    let else_val = self.lower_expr(else_expr, &else_env, loop_ctx)?;
                    let else_exit_bb = self.builder.get_insert_block().unwrap();
                    let else_flows = else_exit_bb.get_terminator().is_none();
                    if else_flows {
                        self.builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(|e| e.to_string())?;
                    }

                    self.builder.position_at_end(merge_bb);

                    // Unit if or both arms terminate early → no phi needed.
                    if is_unit || (!then_flows && !else_flows) {
                        return Ok(unit_value(self.ctx));
                    }

                    // Value-producing if/else: emit phi node.
                    // Struct-typed branches produce PointerValues (their alloca ptrs),
                    // so the phi type must be `ptr`, not the struct type itself.
                    let if_ty_resolved = if_ty.unwrap();
                    let llvm_ty: BasicTypeEnum<'ctx> = if matches!(if_ty_resolved, Ty::Named(_)) {
                        self.ctx.ptr_type(AddressSpace::default()).into()
                    } else {
                        self.llvm_ty(if_ty_resolved)?
                    };
                    let phi = self
                        .builder
                        .build_phi(llvm_ty, "if.val")
                        .map_err(|e| e.to_string())?;
                    if then_flows {
                        let tv = then_tail.unwrap_or_else(|| unit_value(self.ctx));
                        phi.add_incoming(&[(&tv as &dyn BasicValue<'ctx>, then_exit_bb)]);
                    }
                    if else_flows {
                        phi.add_incoming(&[(&else_val as &dyn BasicValue<'ctx>, else_exit_bb)]);
                    }
                    Ok(phi.as_basic_value())
                } else {
                    // No else branch → always Unit.
                    self.builder
                        .build_conditional_branch(cond_val, then_bb, merge_bb)
                        .map_err(|e| e.to_string())?;
                    self.builder.position_at_end(then_bb);
                    let mut then_env = env.clone();
                    self.lower_block(then_block, &mut then_env, loop_ctx)?;
                    if self
                        .builder
                        .get_insert_block()
                        .and_then(|b| b.get_terminator())
                        .is_none()
                    {
                        self.builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(|e| e.to_string())?;
                    }
                    self.builder.position_at_end(merge_bb);
                    Ok(unit_value(self.ctx))
                }
            }

            Expr::While { condition, body, .. } => {
                let header_bb = self.ctx.append_basic_block(self.fn_val, "while.cond");
                let body_bb = self.ctx.append_basic_block(self.fn_val, "while.body");
                let exit_bb = self.ctx.append_basic_block(self.fn_val, "while.exit");

                self.builder
                    .build_unconditional_branch(header_bb)
                    .map_err(|e| e.to_string())?;

                self.builder.position_at_end(header_bb);
                let cond_val = self
                    .lower_expr(condition, env, loop_ctx)?
                    .into_int_value();
                self.builder
                    .build_conditional_branch(cond_val, body_bb, exit_bb)
                    .map_err(|e| e.to_string())?;

                self.builder.position_at_end(body_bb);
                let inner_lctx = LoopCtx { break_bb: exit_bb, continue_bb: header_bb };
                let mut body_env = env.clone();
                self.lower_block(body, &mut body_env, Some(&inner_lctx))?;
                if self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_terminator())
                    .is_none()
                {
                    self.builder
                        .build_unconditional_branch(header_bb)
                        .map_err(|e| e.to_string())?;
                }

                self.builder.position_at_end(exit_bb);
                Ok(unit_value(self.ctx))
            }

            Expr::Loop { body, .. } => {
                let loop_bb = self.ctx.append_basic_block(self.fn_val, "loop.body");
                let exit_bb = self.ctx.append_basic_block(self.fn_val, "loop.exit");

                self.builder
                    .build_unconditional_branch(loop_bb)
                    .map_err(|e| e.to_string())?;
                self.builder.position_at_end(loop_bb);

                let inner_lctx = LoopCtx { break_bb: exit_bb, continue_bb: loop_bb };
                let mut body_env = env.clone();
                self.lower_block(body, &mut body_env, Some(&inner_lctx))?;
                if self
                    .builder
                    .get_insert_block()
                    .and_then(|b| b.get_terminator())
                    .is_none()
                {
                    self.builder
                        .build_unconditional_branch(loop_bb)
                        .map_err(|e| e.to_string())?;
                }

                self.builder.position_at_end(exit_bb);
                Ok(unit_value(self.ctx))
            }

            Expr::Block(block) => {
                let mut block_env = env.clone();
                match self.lower_block(block, &mut block_env, loop_ctx)? {
                    Some(val) => Ok(val),
                    None => Ok(unit_value(self.ctx)),
                }
            }

            Expr::Call { callee, args, .. } => {
                let Expr::Ident(name, _) = callee.as_ref() else {
                    return Err(
                        "only direct function calls supported in PR 32 (no closures or fn pointers)"
                            .to_string(),
                    );
                };
                let callee_fn = self.module.get_function(name).ok_or_else(|| {
                    format!("undefined function `{name}` — declare it before calling")
                })?;
                let arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = args
                    .iter()
                    .map(|a| self.lower_expr(a, env, loop_ctx).map(Into::into))
                    .collect::<Result<_, _>>()?;
                let call_site = self
                    .builder
                    .build_call(callee_fn, &arg_vals, "call")
                    .map_err(|e| e.to_string())?;
                // Void calls return unit_value; value-returning calls return the result.
                Ok(call_site.try_as_basic_value().basic().unwrap_or_else(|| unit_value(self.ctx)))
            }

            Expr::StructLit { name, fields, .. } => {
                let struct_ty = *self.struct_types.get(*name).ok_or_else(|| {
                    format!("ICE: unknown struct `{name}` in lower_expr")
                })?;
                let ptr = self
                    .builder
                    .build_alloca(struct_ty, "struct_tmp")
                    .map_err(|e| e.to_string())?;
                self.lower_struct_lit_into(ptr, struct_ty, name, fields, env, loop_ctx)?;
                Ok(ptr.into())
            }

            Expr::Field { object, field, .. } => {
                let obj_ptr = self.lower_as_ptr(object, env, loop_ctx)?;
                let struct_name = self.struct_name_of(object.span())?;
                let struct_ty = *self.struct_types.get(struct_name).ok_or_else(|| {
                    format!("ICE: struct `{struct_name}` not in struct_types")
                })?;
                let idx = self
                    .types
                    .struct_field_index(struct_name, field)
                    .ok_or_else(|| {
                        format!("ICE: field `{field}` not found in struct `{struct_name}`")
                    })? as u32;
                let field_ty = self.llvm_ty(
                    self.types
                        .struct_field_type(struct_name, field)
                        .ok_or_else(|| {
                            format!(
                                "ICE: field `{field}` type not found in struct `{struct_name}`"
                            )
                        })?,
                )?;
                let gep = self
                    .builder
                    .build_struct_gep(struct_ty, obj_ptr, idx, "field_ptr")
                    .map_err(|e| e.to_string())?;
                self.builder
                    .build_load(field_ty, gep, "field")
                    .map_err(|e| e.to_string())
            }

            Expr::MethodCall { object, method, args, .. } => {
                let struct_name = self.struct_name_of(object.span())?;
                let mangled = format!("{struct_name}_{method}");
                let callee = self.module.get_function(&mangled).ok_or_else(|| {
                    format!("ICE: method `{mangled}` not declared in module")
                })?;
                let self_ptr = self.lower_as_ptr(object, env, loop_ctx)?;
                let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> =
                    vec![self_ptr.into()];
                for a in args {
                    call_args.push(self.lower_expr(a, env, loop_ctx)?.into());
                }
                let call = self
                    .builder
                    .build_call(callee, &call_args, "method_call")
                    .map_err(|e| e.to_string())?;
                Ok(call.try_as_basic_value().basic().unwrap_or_else(|| unit_value(self.ctx)))
            }

            _ => Err(format!(
                "expression not supported in this PR (at byte {}): \
                 match, for, cast, borrow, and closures land in later PRs",
                expr.span().start
            )),
        }
    }

    // ── Binary op lowering ────────────────────────────────────────────────────

    fn lower_binary(
        &self,
        op: BinOp,
        lv: BasicValueEnum<'ctx>,
        rv: BasicValueEnum<'ctx>,
        operand_ty: &Ty,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        match operand_ty {
            Ty::I32 => {
                let l = lv.into_int_value();
                let r = rv.into_int_value();
                Ok(match op {
                    BinOp::Add => self
                        .builder
                        .build_int_add(l, r, "add")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Sub => self
                        .builder
                        .build_int_sub(l, r, "sub")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Mul => self
                        .builder
                        .build_int_mul(l, r, "mul")
                        .map_err(|e| e.to_string())?
                        .into(),
                    // Div/Mod: runtime zero-divisor check → calls libc abort() (pillar 1).
                    BinOp::Div => self.emit_int_div_or_rem(l, r, false)?,
                    BinOp::Mod => self.emit_int_div_or_rem(l, r, true)?,
                    BinOp::Eq => self
                        .builder
                        .build_int_compare(IntPredicate::EQ, l, r, "eq")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Ne => self
                        .builder
                        .build_int_compare(IntPredicate::NE, l, r, "ne")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Lt => self
                        .builder
                        .build_int_compare(IntPredicate::SLT, l, r, "lt")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Gt => self
                        .builder
                        .build_int_compare(IntPredicate::SGT, l, r, "gt")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Le => self
                        .builder
                        .build_int_compare(IntPredicate::SLE, l, r, "le")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Ge => self
                        .builder
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
                    BinOp::Add => self
                        .builder
                        .build_float_add(l, r, "fadd")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Sub => self
                        .builder
                        .build_float_sub(l, r, "fsub")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Mul => self
                        .builder
                        .build_float_mul(l, r, "fmul")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Div => self
                        .builder
                        .build_float_div(l, r, "fdiv")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Mod => self
                        .builder
                        .build_float_rem(l, r, "frem")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Eq => self
                        .builder
                        .build_float_compare(FloatPredicate::OEQ, l, r, "feq")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Ne => self
                        .builder
                        .build_float_compare(FloatPredicate::ONE, l, r, "fne")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Lt => self
                        .builder
                        .build_float_compare(FloatPredicate::OLT, l, r, "flt")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Gt => self
                        .builder
                        .build_float_compare(FloatPredicate::OGT, l, r, "fgt")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Le => self
                        .builder
                        .build_float_compare(FloatPredicate::OLE, l, r, "fle")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Ge => self
                        .builder
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
                    BinOp::And => self
                        .builder
                        .build_and(l, r, "and")
                        .map_err(|e| e.to_string())?
                        .into(),
                    BinOp::Or => self
                        .builder
                        .build_or(l, r, "or")
                        .map_err(|e| e.to_string())?
                        .into(),
                    _ => return Err(format!("operator {op:?} not supported for bool")),
                })
            }
            ty => Err(format!("binary op on unsupported type: {ty}")),
        }
    }

    /// Emit an i32 div or rem with a runtime zero-divisor check.
    /// Zero divisor → calls libc `abort()` and marks the block unreachable.
    /// Pillar 1: explicit runtime panic, never silent UB.
    fn emit_int_div_or_rem(
        &self,
        l: IntValue<'ctx>,
        r: IntValue<'ctx>,
        is_rem: bool,
    ) -> Result<BasicValueEnum<'ctx>, String> {
        let abort_fn = self.get_or_declare_abort();
        // Guard 1: divide by zero.
        let zero = self.ctx.i32_type().const_zero();
        let is_zero = self
            .builder
            .build_int_compare(IntPredicate::EQ, r, zero, "divz")
            .map_err(|e| e.to_string())?;
        // Guard 2: INT_MIN / -1 is signed overflow → LLVM poison.
        // -1 as u64 gives the correct bit pattern for const_int on an i32 type.
        let neg_one = self.ctx.i32_type().const_int(u64::MAX, false);
        let int_min = self.ctx.i32_type().const_int(i32::MIN as u64, false);
        let r_is_neg_one = self
            .builder
            .build_int_compare(IntPredicate::EQ, r, neg_one, "neg1")
            .map_err(|e| e.to_string())?;
        let l_is_int_min = self
            .builder
            .build_int_compare(IntPredicate::EQ, l, int_min, "minval")
            .map_err(|e| e.to_string())?;
        let is_overflow = self
            .builder
            .build_and(r_is_neg_one, l_is_int_min, "overflow")
            .map_err(|e| e.to_string())?;
        let is_bad = self
            .builder
            .build_or(is_zero, is_overflow, "divbad")
            .map_err(|e| e.to_string())?;

        let abort_bb = self.ctx.append_basic_block(self.fn_val, "div.abort");
        let ok_bb = self.ctx.append_basic_block(self.fn_val, "div.ok");
        self.builder
            .build_conditional_branch(is_bad, abort_bb, ok_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(abort_bb);
        self.builder.build_call(abort_fn, &[], "").map_err(|e| e.to_string())?;
        self.builder.build_unreachable().map_err(|e| e.to_string())?;

        self.builder.position_at_end(ok_bb);
        if is_rem {
            Ok(self
                .builder
                .build_int_signed_rem(l, r, "rem")
                .map_err(|e| e.to_string())?
                .into())
        } else {
            Ok(self
                .builder
                .build_int_signed_div(l, r, "div")
                .map_err(|e| e.to_string())?
                .into())
        }
    }

    fn get_or_declare_abort(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("abort") {
            return f;
        }
        let ty = self.ctx.void_type().fn_type(&[], false);
        let f = self.module.add_function("abort", ty, Some(Linkage::External));
        // Mark noreturn so LLVM knows code after the call is unreachable (not UB-shaped).
        let noreturn =
            self.ctx.create_enum_attribute(Attribute::get_named_enum_kind_id("noreturn"), 0);
        f.add_attribute(AttributeLoc::Function, noreturn);
        f
    }
}

// ─── Module-level orchestration ───────────────────────────────────────────────

fn lower_to_module<'ctx>(
    ctx: &'ctx Context,
    ast: &Ast<'_>,
    types: &InferResult,
) -> Result<Module<'ctx>, String> {
    let module = ctx.create_module("main");

    // Pass 0: register LLVM named struct types.
    // Sub-pass 0a: create opaque types so forward references resolve.
    let mut struct_types: HashMap<String, StructType<'ctx>> = HashMap::new();
    for item in &ast.items {
        if let Item::Struct(def) = item {
            struct_types.insert(def.name.to_string(), ctx.opaque_struct_type(def.name));
        }
    }
    // Sub-pass 0b: set struct bodies (all names now registered).
    for item in &ast.items {
        if let Item::Struct(def) = item {
            let field_names = types
                .struct_field_names(def.name)
                .ok_or_else(|| format!("ICE: struct `{}` not in InferResult", def.name))?;
            let field_tys: Vec<BasicTypeEnum<'ctx>> = field_names
                .iter()
                .map(|fname| {
                    let ty = types.struct_field_type(def.name, fname).ok_or_else(|| {
                        format!(
                            "ICE: field `{fname}` type not found in struct `{}`",
                            def.name
                        )
                    })?;
                    resolve_ty(ty, ctx, &struct_types)
                })
                .collect::<Result<_, _>>()?;
            struct_types[def.name].set_body(&field_tys, false);
        }
    }

    // Pass 1: declare all function and method signatures before lowering any body.
    // Required for forward calls and mutual recursion.
    for item in &ast.items {
        match item {
            Item::Function(func) => declare_function_sig(func, ctx, &module, &struct_types)?,
            Item::Impl(block) => {
                for method in &block.methods {
                    declare_method_sig(block.type_name, method, ctx, &module, &struct_types)?;
                }
            }
            Item::Struct(_) => {}
        }
    }

    // Pass 2: lower function and method bodies (all callees already visible in module).
    for item in &ast.items {
        match item {
            Item::Function(func) => lower_function(func, types, ctx, &module, &struct_types)?,
            Item::Impl(block) => {
                for method in &block.methods {
                    lower_method(block.type_name, method, types, ctx, &module, &struct_types)?;
                }
            }
            Item::Struct(_) => {}
        }
    }

    Ok(module)
}

/// Pass 1 helper: add the LLVM function declaration (signature only, no body).
fn declare_function_sig<'ctx>(
    func: &FunctionDef<'_>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    struct_types: &HashMap<String, StructType<'ctx>>,
) -> Result<(), String> {
    let param_types: Vec<BasicMetadataTypeEnum<'ctx>> = func
        .params
        .iter()
        .map(|p| basic_type_from_ast(&p.ty, ctx, struct_types).map(Into::into))
        .collect::<Result<_, _>>()?;
    let fn_type = match func.return_ty.as_ref() {
        None => ctx.void_type().fn_type(&param_types, false),
        Some(ty) => basic_type_from_ast(ty, ctx, struct_types)?.fn_type(&param_types, false),
    };
    module.add_function(func.name, fn_type, None);
    Ok(())
}

/// Pass 1 helper: declare a method from an impl block with a mangled name.
///
/// Mangling: `{TypeName}_{method_name}` (e.g. `Point_length`).
/// Self receiver (first param with `Type::SelfTy`) is replaced by a pointer to the struct.
/// Associated functions (no SelfTy receiver) are handled like free functions.
fn declare_method_sig<'ctx>(
    type_name: &str,
    method: &FunctionDef<'_>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    struct_types: &HashMap<String, StructType<'ctx>>,
) -> Result<(), String> {
    let mangled = format!("{type_name}_{}", method.name);
    let has_self = method
        .params
        .first()
        .is_some_and(|p| matches!(p.ty, Type::SelfTy(_)));

    let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = Vec::new();
    if has_self {
        if !struct_types.contains_key(type_name) {
            return Err(format!("ICE: struct `{type_name}` not found for method declaration"));
        }
        param_types.push(ctx.ptr_type(AddressSpace::default()).into());
    }
    let explicit_params = if has_self { &method.params[1..] } else { &method.params[..] };
    for p in explicit_params {
        param_types.push(basic_type_from_ast(&p.ty, ctx, struct_types).map(Into::into)?);
    }

    let fn_type = match method.return_ty.as_ref() {
        None => ctx.void_type().fn_type(&param_types, false),
        Some(ty) => basic_type_from_ast(ty, ctx, struct_types)?.fn_type(&param_types, false),
    };
    module.add_function(&mangled, fn_type, None);
    Ok(())
}

/// Construct an `FnLower` for `func` and drive the lowering of its body.
fn lower_function<'ctx, 'b, 'src>(
    func: &FunctionDef<'src>,
    types: &'b InferResult,
    ctx: &'ctx Context,
    module: &'b Module<'ctx>,
    struct_types: &'b HashMap<String, StructType<'ctx>>,
) -> Result<(), String> {
    // Retrieve the pre-declared LLVM function from pass 1.
    let fn_val = module
        .get_function(func.name)
        .ok_or_else(|| format!("ICE: `{}` not pre-declared in pass 1", func.name))?;

    let entry = ctx.append_basic_block(fn_val, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry);

    let mut lower = FnLower {
        builder, ctx, module, types, fn_val, struct_types,
        alloca_slots: HashMap::new(),
    };
    let mut env: CodegenEnv<'ctx, 'src> = HashMap::new();

    // Phase 1: emit all allocas at the entry block top (canonical mem2reg form).
    // Params first, then all body lets (including shadowed and nested ones).
    let mut param_alloca_entries: Vec<(&'src str, PointerValue<'ctx>, BasicTypeEnum<'ctx>)> =
        Vec::new();
    for param in &func.params {
        let llvm_ty = basic_type_from_ast(&param.ty, ctx, struct_types)?;
        let ptr = lower
            .builder
            .build_alloca(llvm_ty, param.name)
            .map_err(|e| e.to_string())?;
        param_alloca_entries.push((param.name, ptr, llvm_ty));
    }
    lower.emit_allocas(&func.body.stmts)?;

    // Phase 2: store param values into their allocas (after all alloca instructions).
    for (i, (name, ptr, llvm_ty)) in param_alloca_entries.into_iter().enumerate() {
        let param_val = fn_val
            .get_nth_param(i as u32)
            .ok_or_else(|| format!("ICE: missing param {i} for `{}`", func.name))?;
        lower.builder.build_store(ptr, param_val).map_err(|e| e.to_string())?;
        env.insert(name, (ptr, llvm_ty));
    }

    // Phase 3: lower body statements.
    for stmt in &func.body.stmts {
        lower.lower_stmt(stmt, &mut env, None)?;
        if lower
            .builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
        {
            break; // explicit return/break terminated the block; skip dead code
        }
    }

    // Phase 4: tail expression → return instruction (only when no explicit terminator).
    if lower
        .builder
        .get_insert_block()
        .and_then(|b| b.get_terminator())
        .is_none()
    {
        if func.return_ty.is_none() {
            lower.builder.build_return(None).map_err(|e| e.to_string())?;
        } else {
            match &func.body.tail {
                Some(tail) => {
                    let val = lower.lower_expr(tail, &env, None)?;
                    lower.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
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

/// Drive lowering of a method body. Mirrors `lower_function` but:
/// - Uses the mangled name to look up the pre-declared LLVM function.
/// - Inserts the self pointer as the first environment entry under `"self"`.
fn lower_method<'ctx, 'b, 'src>(
    type_name: &str,
    method: &FunctionDef<'src>,
    types: &'b InferResult,
    ctx: &'ctx Context,
    module: &'b Module<'ctx>,
    struct_types: &'b HashMap<String, StructType<'ctx>>,
) -> Result<(), String> {
    let mangled = format!("{type_name}_{}", method.name);
    let fn_val = module
        .get_function(&mangled)
        .ok_or_else(|| format!("ICE: `{mangled}` not pre-declared in pass 1"))?;

    let entry = ctx.append_basic_block(fn_val, "entry");
    let builder = ctx.create_builder();
    builder.position_at_end(entry);

    let mut lower = FnLower {
        builder, ctx, module, types, fn_val, struct_types,
        alloca_slots: HashMap::new(),
    };
    let mut env: CodegenEnv<'ctx, 'src> = HashMap::new();

    let has_self = method
        .params
        .first()
        .is_some_and(|p| matches!(p.ty, Type::SelfTy(_)));

    // Track how many LLVM params have been consumed so far.
    let mut llvm_param_idx: u32 = 0;

    // Phase 1: register self and allocas.
    //
    // The self receiver is a %StructType* pointer passed as LLVM param 0.
    // We insert it directly into env so that `lower_as_ptr(Ident("self"))` returns
    // the pointer itself — no intermediate alloca needed. The BasicTypeEnum entry
    // is the struct type so that any load of "self" (e.g. a tail `self` expression)
    // would load the full struct value.
    if has_self {
        let struct_ty = *struct_types
            .get(type_name)
            .ok_or_else(|| format!("ICE: struct `{type_name}` not in struct_types"))?;
        let self_llvm_param = fn_val
            .get_nth_param(0)
            .ok_or_else(|| "ICE: missing self param".to_string())?
            .into_pointer_value();
        env.insert("self", (self_llvm_param, struct_ty.into()));
        llvm_param_idx = 1;
    }

    let explicit_params = if has_self { &method.params[1..] } else { &method.params[..] };
    let mut param_alloca_entries: Vec<(&'src str, PointerValue<'ctx>, BasicTypeEnum<'ctx>)> =
        Vec::new();
    for param in explicit_params {
        let llvm_ty = basic_type_from_ast(&param.ty, ctx, struct_types)?;
        let ptr = lower
            .builder
            .build_alloca(llvm_ty, param.name)
            .map_err(|e| e.to_string())?;
        param_alloca_entries.push((param.name, ptr, llvm_ty));
    }
    lower.emit_allocas(&method.body.stmts)?;

    // Phase 2: store explicit param values.
    for (name, ptr, llvm_ty) in param_alloca_entries {
        let param_val = fn_val
            .get_nth_param(llvm_param_idx)
            .ok_or_else(|| format!("ICE: missing param {llvm_param_idx} for `{mangled}`"))?;
        lower.builder.build_store(ptr, param_val).map_err(|e| e.to_string())?;
        env.insert(name, (ptr, llvm_ty));
        llvm_param_idx += 1;
    }

    // Phase 3: lower body statements.
    for stmt in &method.body.stmts {
        lower.lower_stmt(stmt, &mut env, None)?;
        if lower
            .builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
        {
            break;
        }
    }

    // Phase 4: tail expression → return.
    if lower
        .builder
        .get_insert_block()
        .and_then(|b| b.get_terminator())
        .is_none()
    {
        if method.return_ty.is_none() {
            lower.builder.build_return(None).map_err(|e| e.to_string())?;
        } else {
            match &method.body.tail {
                Some(tail) => {
                    let val = lower.lower_expr(tail, &env, None)?;
                    lower.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
                }
                None => {
                    return Err(format!(
                        "method `{mangled}`: body has no terminator and no tail expression"
                    ));
                }
            }
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

// ─── Type helpers ─────────────────────────────────────────────────────────────

/// Resolve a typechecker `Ty` to an LLVM `BasicTypeEnum`.
///
/// Handles both primitive types and named struct types. Used by Pass 0b
/// (setting struct field layouts) and by `FnLower::llvm_ty` (body lowering).
fn resolve_ty<'ctx>(
    ty: &Ty,
    ctx: &'ctx Context,
    struct_types: &HashMap<String, StructType<'ctx>>,
) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Ty::Named(name) => struct_types
            .get(name.as_str())
            .map(|st| BasicTypeEnum::StructType(*st))
            .ok_or_else(|| format!("ICE: unknown struct type `{name}` in codegen")),
        other => basic_type(other, ctx),
    }
}

fn basic_type<'ctx>(ty: &Ty, ctx: &'ctx Context) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Ty::I32 => Ok(ctx.i32_type().into()),
        Ty::F64 => Ok(ctx.f64_type().into()),
        Ty::Bool => Ok(ctx.bool_type().into()),
        other => Err(format!(
            "type `{other}` is not yet lowerable; only i32, f64, bool, and struct types are \
             supported — consider filing a feature request or using a supported type"
        )),
    }
}

fn basic_type_from_ast<'ctx>(
    ty: &Type<'_>,
    ctx: &'ctx Context,
    struct_types: &HashMap<String, StructType<'ctx>>,
) -> Result<BasicTypeEnum<'ctx>, String> {
    match ty {
        Type::Named { name: "i32", .. } => Ok(ctx.i32_type().into()),
        Type::Named { name: "f64", .. } => Ok(ctx.f64_type().into()),
        Type::Named { name: "bool", .. } => Ok(ctx.bool_type().into()),
        Type::Named { name, .. } => struct_types
            .get(*name)
            .map(|st| BasicTypeEnum::StructType(*st))
            .ok_or_else(|| format!("unknown type `{name}` in codegen")),
        // SelfTy is only valid as a receiver, never as a value-type param in declare
        other => Err(format!("type not supported in codegen: {other:?}")),
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

    // ── Struct / method tests ─────────────────────────────────────────────────

    /// T12 — struct instantiation + field read: `Point { x=3, y=4 }.x == 3` (JIT).
    /// Fields are stored in declaration order; literal order is independent.
    #[test]
    fn test_struct_field_read_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { let p = Point { x = 3, y = 4 }; p.x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 3, "expected p.x == 3, got {result}");
    }

    /// T13 — struct literal in reverse field order: `Point { y=4, x=3 }.x == 3` (JIT).
    /// Proves GEP uses declaration order (not literal order).
    #[test]
    fn test_struct_lit_reverse_order_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { let p = Point { y = 4, x = 3 }; p.x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 3, "expected p.x == 3 (declaration order), got {result}");
    }

    /// T14 — method dispatch: `Counter_inc` mutates the caller's struct via self pointer (JIT).
    /// Proves the self-as-pointer calling convention: writes inside the method are visible
    /// in the caller after the call returns.
    #[test]
    fn test_method_self_pointer_mutation_jit() {
        let ctx = Context::create();
        let src = "struct Counter { value: i32 } \
                   impl Counter { fn inc(self) { self.value = self.value + 1; } } \
                   fn f() -> i32 { let c = Counter { value = 0 }; c.inc(); c.value }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 1, "expected c.value == 1 after c.inc(), got {result}");
    }

    /// T15 — same-name methods on two types: `Point_sum` and `Vec2_sum` must not collide (JIT).
    /// Proves that the `{TypeName}_{method_name}` mangling scheme isolates method namespaces.
    #[test]
    fn test_same_name_methods_different_types_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   struct Vec2 { x: i32, y: i32 } \
                   impl Point { fn sum(self) -> i32 { self.x + self.y } } \
                   impl Vec2  { fn sum(self) -> i32 { self.x * self.y } } \
                   fn f() -> i32 { \
                       let p = Point { x = 3, y = 4 }; \
                       let v = Vec2  { x = 3, y = 4 }; \
                       p.sum() + v.sum() \
                   }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        // Point: 3+4=7, Vec2: 3*4=12, total=19
        assert_eq!(result, 19, "expected Point.sum()+Vec2.sum() == 19, got {result}");
    }

    /// T16 — struct literal as direct method receiver (no intermediate `let`): JIT.
    /// Exercises the `lower_as_ptr(StructLit)` → fresh alloca → self pointer path.
    #[test]
    fn test_struct_lit_direct_method_receiver_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   impl Point { fn sum(self) -> i32 { self.x + self.y } } \
                   fn f() -> i32 { Point { x = 1, y = 2 }.sum() }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 3, "expected Point{{1,2}}.sum() == 3, got {result}");
    }

    /// T17 — method returns struct field: `get_x` reads self.x through the self pointer (JIT).
    #[test]
    fn test_method_field_read_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   impl Point { fn get_x(self) -> i32 { self.x } } \
                   fn f() -> i32 { let p = Point { x = 42, y = 0 }; p.get_x() }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 42, "expected p.get_x() == 42, got {result}");
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

    /// T18 — block-wrapped struct lit in let binding: tail-position transparency.
    /// `let p = { Point { x=7, y=9 } }; p.x` must lower correctly — the Block wrapper
    /// must not cause pointer bits to be stored into the struct alloca.
    #[test]
    fn test_block_wrapped_struct_let_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { let p = { Point { x = 7, y = 9 } }; p.x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 7, "expected block-wrapped struct let p.x == 7, got {result}");
    }

    /// T19 — block-wrapped struct lit as field access receiver: tail-position transparency.
    /// `{ Point { x=7, y=9 } }.x` — the Block wrapper must not produce a pointer-to-pointer.
    #[test]
    fn test_block_wrapped_struct_field_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { { Point { x = 7, y = 9 } }.x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 7, "expected {{ Point {{x=7,y=9}} }}.x == 7, got {result}");
    }

    /// T20 — block-wrapped struct lit as method receiver: tail-position transparency.
    /// `{ Point { x=7, y=9 } }.sum()` — self pointer must point to a real struct, not a
    /// pointer-to-pointer.
    #[test]
    fn test_block_wrapped_struct_method_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   impl Point { fn sum(self) -> i32 { self.x + self.y } } \
                   fn f() -> i32 { { Point { x = 7, y = 9 } }.sum() }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 16, "expected {{ Point{{7,9}} }}.sum() == 16, got {result}");
    }

    /// T21 — if/else producing a struct value, stored in let binding.
    /// `let p = if true { Point{x=7,y=9} } else { Point{x=0,y=0} }; p.x` must
    /// use a phi over pointer type, then load+copy into the dest alloca.
    #[test]
    fn test_if_else_struct_value_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   fn f() -> i32 { \
                       let p = if true { Point { x = 7, y = 9 } } else { Point { x = 0, y = 0 } }; \
                       p.x \
                   }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine.get_function::<unsafe extern "C" fn() -> i32>("f").unwrap().call()
        };
        assert_eq!(result, 7, "expected if/else struct p.x == 7, got {result}");
    }

    /// T22 — struct-as-field: Entity containing Point; two-level field read (forward order).
    /// Exercises the Pass 0b fix: resolve_ty handles Ty::Named for struct-typed fields.
    #[test]
    fn test_struct_as_field_nested_read_jit() {
        let ctx = Context::create();
        let src = "struct Point { x: i32, y: i32 } \
                   struct Entity { position: Point, id: i32 } \
                   fn make() -> i32 { \
                       let e = Entity { position = Point { x = 3, y = 4 }, id = 10 }; \
                       e.position.x + e.position.y + e.id \
                   }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("make")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 17, "expected e.position.x + e.position.y + e.id == 17, got {result}");
    }

    /// T23 — struct-as-field: Entity declared before Point (reverse order).
    /// Confirms Pass 0a/0b forward-reference handling is order-independent.
    #[test]
    fn test_struct_as_field_reverse_decl_order_jit() {
        let ctx = Context::create();
        let src = "struct Entity { position: Point, id: i32 } \
                   struct Point { x: i32, y: i32 } \
                   fn make() -> i32 { \
                       let e = Entity { position = Point { x = 3, y = 4 }, id = 10 }; \
                       e.position.x + e.position.y + e.id \
                   }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("make")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 17, "expected reverse-order e.position.x + e.position.y + e.id == 17, got {result}");
    }

    /// T27 — i32::MIN literal: `-2147483648` must compile and return i32::MIN end-to-end.
    /// Verifies the codegen fold in lower_expr for Neg(Literal::Integer(2147483648)).
    #[test]
    fn test_i32_min_literal_jit() {
        let ctx = Context::create();
        let src = "fn f() -> i32 { -2147483648 }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("f")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, i32::MIN, "expected i32::MIN (-2147483648), got {result}");
    }

    /// T24 — nested-block shadow must not corrupt the outer binding.
    /// `let x = 1; let y = { let x = 2; x }; x` — outer x must remain 1.
    #[test]
    fn test_shadow_nested_block_does_not_corrupt_outer_jit() {
        let ctx = Context::create();
        let src = "fn f() -> i32 { let x = 1; let y = { let x = 2; x }; x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("f")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 1, "outer x should remain 1 after inner block shadows it, got {result}");
    }

    /// T25 — same-block shadow: the second binding overwrites the name.
    /// `let x = 1; let x = 2; x` — must return 2.
    #[test]
    fn test_shadow_same_block_jit() {
        let ctx = Context::create();
        let src = "fn f() -> i32 { let x = 1; let x = 2; x }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("f")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 2, "shadow should make x == 2, got {result}");
    }

    /// T26 — capture before shadow: y captures x before the shadow overwrites x.
    /// `let x = 1; let y = x; let x = 2; y` — must return 1.
    #[test]
    fn test_shadow_capture_before_shadow_jit() {
        let ctx = Context::create();
        let src = "fn f() -> i32 { let x = 1; let y = x; let x = 2; y }";
        let module = compile_to_module(&ctx, src);
        let engine = module
            .create_jit_execution_engine(OptimizationLevel::None)
            .expect("JIT engine creation failed");
        let result: i32 = unsafe {
            engine
                .get_function::<unsafe extern "C" fn() -> i32>("f")
                .expect("function not found in JIT module")
                .call()
        };
        assert_eq!(result, 1, "y should hold x's value before the shadow (1), got {result}");
    }
}
