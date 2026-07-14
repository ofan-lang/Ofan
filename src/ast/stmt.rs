use crate::lexer::Span;
use super::{BinOp, Expr, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt<'src> {
    /// `let [mut] name[: Type] = init;`
    Let {
        mutable: bool,
        name: &'src str,
        name_span: Span,
        ty: Option<Type<'src>>,
        init: Box<Expr<'src>>,
        span: Span,
    },
    /// `const name: Type = init;`
    Const {
        name: &'src str,
        name_span: Span,
        ty: Type<'src>,
        init: Box<Expr<'src>>,
        span: Span,
    },
    /// `return [expr];`
    Return { value: Option<Box<Expr<'src>>>, span: Span },
    /// `break [expr];` — `expr` only valid inside `loop`
    Break { value: Option<Box<Expr<'src>>>, span: Span },
    /// `continue;`
    Continue { span: Span },
    /// `lvalue [op]= rhs;`  —  `op` is None for plain `=`, Some for compound
    Assign {
        target: Box<Expr<'src>>,
        op: Option<BinOp>,
        value: Box<Expr<'src>>,
        span: Span,
    },
    /// Expression used as a statement.
    /// `has_semicolon: true`  — expression statement (`expr;`), value discarded.
    /// `has_semicolon: false` — tail expression (`expr` with no `;`), becomes the
    ///   block's return value. Invariant: this variant only appears in `Block::stmts`
    ///   when `has_semicolon` is `true`; a semicolon-less expr is extracted into
    ///   `Block::tail` by `parse_block` and never left in the `stmts` vec.
    Expr { expr: Box<Expr<'src>>, has_semicolon: bool, span: Span },
}
