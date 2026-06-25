use crate::ast::Ast;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("type mismatch: expected {expected}, found {found}")]
    Mismatch { expected: String, found: String },
    #[error("undefined variable: {name}")]
    UndefinedVariable { name: String },
}

/// Run type and lifetime inference over a parsed AST.
pub fn infer(_ast: &Ast) -> Result<(), TypeError> {
    // TODO: implement type inference and lifetime inference engine
    Ok(())
}
