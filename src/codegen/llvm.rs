use inkwell::{
    context::Context,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    OptimizationLevel,
};
use std::path::Path;

/// LLVM compilation context for one compiler invocation.
pub struct LlvmContext {
    inner: Context,
}

impl LlvmContext {
    pub fn new() -> Self {
        Self { inner: Context::create() }
    }

    // TODO: promote link_object/emit errors to a typed CodegenError enum (consistency with TypeError).

    /// Emit a hardcoded `fn main() -> i32 { 0 }` binary to `out`.
    /// PR-30 infrastructure: proves IR → obj → link plumbing without real AST lowering.
    pub fn emit_hardcoded_main(&self, out: &Path) -> Result<(), String> {
        Target::initialize_x86(&InitializationConfig::default()); // x86-only for now; extend when multi-target lands

        let module = self.inner.create_module("main");
        let builder = self.inner.create_builder();

        let i32_type = self.inner.i32_type();
        let fn_type = i32_type.fn_type(&[], false);
        let fn_val = module.add_function("main", fn_type, None);
        let entry = self.inner.append_basic_block(fn_val, "entry");
        builder.position_at_end(entry);
        builder
            .build_return(Some(&i32_type.const_int(0, false)))
            .map_err(|e| e.to_string())?;

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
        tm.write_to_file(&module, FileType::Object, &obj)
            .map_err(|e| e.to_string())?;

        link_object(&obj, out)?;
        if let Err(e) = std::fs::remove_file(&obj) {
            eprintln!("ofan: warning: could not remove {}: {e}", obj.display());
        }
        Ok(())
    }
}

impl Default for LlvmContext {
    fn default() -> Self {
        Self::new()
    }
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
