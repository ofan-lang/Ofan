pub mod error;
pub use error::TypeError;

pub mod ty;
pub use ty::Ty;

pub(crate) mod env;
pub(crate) mod infer;

use crate::ast::Ast;
use crate::lexer::token::Span;
use std::collections::HashMap;

/// Opaque result returned by a successful inference pass.
///
/// Internal representation can grow (new fields, region solutions, etc.) without
/// changing the public type — callers access it only through `type_of`.
pub struct InferResult {
    pub(crate) type_map: HashMap<Span, Ty>,
    // PHASE2: pub(crate) region_solution: RegionSolution,
}

impl InferResult {
    /// Look up the inferred type for the expression at the given source span.
    /// Returns `None` for spans not recorded (e.g. deferred nodes typed as `Ty::Error`
    /// that were not entered into the map).
    pub fn type_of(&self, span: Span) -> Option<&Ty> {
        self.type_map.get(&span)
    }
}

/// Run type inference and checking over a parsed AST.
///
/// Returns `Ok(InferResult)` if no fatal type errors are found.
/// Returns `Err(errors)` with every error collected in a single pass — the
/// checker is non-fatal by design so the user sees all problems at once.
/// Non-fatal `TypeError::Deferred` diagnostics are included in the `Err` vec
/// only when fatal errors are also present; otherwise they are silently dropped
/// (the program type-checks, but some constructs weren't fully verified).
pub fn infer(ast: &Ast<'_>) -> Result<InferResult, Vec<TypeError>> {
    infer::run(ast)
}
