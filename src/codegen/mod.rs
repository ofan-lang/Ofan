// LLVM codegen via inkwell.
// Gated behind the `codegen` feature flag — requires LLVM dev libraries at build time.
// Enable with: cargo build --features codegen

#[cfg(feature = "codegen")]
/// Name of the Ofan entry function as it appears in the emitted LLVM IR.
/// Used by the linker invocation (/ENTRY: on Windows) and by the entry-present
/// validation in lower_to_module — the two must always agree.
pub(crate) const ENTRY_FN: &str = "main";

#[cfg(feature = "codegen")]
pub mod llvm;
