use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodegenError {
    /// Inkwell/LLVM builder error propagated from the LLVM C API.
    #[error("LLVM error: {0}")]
    Llvm(String),

    /// A language construct is syntactically and type-correct but not yet
    /// lowerable to LLVM IR. `feature` names the construct; `byte` is its
    /// byte offset in the source file.
    #[error(
        "not yet lowerable: {feature} (at byte {byte}) — \
         this construct is planned; track progress in docs/PROGRESS.md"
    )]
    NotYetLowered { feature: &'static str, byte: usize },

    /// Internal compiler error — a typechecker invariant was violated at
    /// codegen time. Always a compiler bug, never a user error.
    #[error(
        "internal compiler error: {0} — \
         please file a bug at github.com/ofan-lang/Ofan/issues"
    )]
    Ice(String),

    /// The source file has no `fn main` entry point.
    #[error(
        "no entry function found — add `fn main() -> i32 {{ ... }}` to your \
         source file (§6 of docs/SYNTAX_SPEC.md)"
    )]
    EntryFnMissing,
}

/// Allows `?` to propagate bare `String` errors as `Ice` — used for ICE-style
/// `ok_or_else(|| format!("ICE: ..."))` sites throughout `llvm.rs`.
/// Any site that should be a different variant must convert explicitly.
impl From<String> for CodegenError {
    fn from(s: String) -> Self {
        CodegenError::Ice(s)
    }
}
