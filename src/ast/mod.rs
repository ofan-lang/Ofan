use crate::lexer::Span;

mod ty;
mod pattern;
mod expr;
mod stmt;
mod item;

pub use ty::{Type, RefRegion};
pub use pattern::Pattern;
pub use expr::{Expr, MatchArm, StructFieldInit, BinOp, UnaryOp, BorrowKind};
pub use stmt::Stmt;
pub use item::{Ast, Item, FunctionDef, ImplBlock, Param, StructDef, StructField, CopyMove, EnumDef, EnumVariant};

// ─── Shared types ─────────────────────────────────────────────────────────────
// Block and Literal live here, not in a submodule, because each is needed by
// multiple siblings (Block: expr/stmt/item; Literal: expr/pattern). A dedicated
// sixth file for two small leaf types would be more churn than benefit.

/// A braced block: zero or more statements followed by an optional tail expression.
/// The tail expression (no trailing `;`) is the block's value.
#[derive(Debug, Clone, PartialEq)]
pub struct Block<'src> {
    pub stmts: Vec<Stmt<'src>>,
    pub tail: Option<Box<Expr<'src>>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal<'src> {
    Integer(i64),
    Float(f64),
    Bool(bool),
    /// Raw source slice — escape sequences not yet decoded (§15 deferred)
    Str(&'src str),
    Char(char),
}
