// LLVM codegen via inkwell.
// Gated behind the `codegen` feature flag — requires LLVM dev libraries at build time.
// Enable with: cargo build --features codegen

#[cfg(feature = "codegen")]
pub mod llvm;
