use inkwell::context::Context;

/// LLVM compilation context for one compiler invocation.
pub struct LlvmContext {
    inner: Context,
}

impl LlvmContext {
    pub fn new() -> Self {
        Self { inner: Context::create() }
    }
}

impl Default for LlvmContext {
    fn default() -> Self {
        Self::new()
    }
}
