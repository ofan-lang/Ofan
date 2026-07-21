use crate::lexer::Span;
use super::{Block, Literal, Pattern, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,       // `-`
    Not,       // `!`
    BitNot,    // `~`
    Borrow,    // `&`
    BorrowMut, // `&mut`
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Shared, // `&`
    Mut,    // `&mut`
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add, Sub, Mul, Div, Mod,
    // Comparison
    Eq, Ne, Lt, Gt, Le, Ge,
    // Logical
    And, Or,
    // Bitwise
    BitAnd, BitOr, BitXor, Shl, Shr,
    // Fallback (§12 `?:`) — Option<T> only
    Fallback,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr<'src> {
    Literal(Literal<'src>, Span),
    Ident(&'src str, Span),

    // --- Unary ---
    Unary { op: UnaryOp, expr: Box<Expr<'src>>, span: Span },

    // --- Binary (includes `?:` fallback, §12) ---
    Binary { op: BinOp, left: Box<Expr<'src>>, right: Box<Expr<'src>>, span: Span },

    // --- Postfix ---
    /// `callee(arg1, arg2, ...)` — free function call
    Call { callee: Box<Expr<'src>>, args: Vec<Expr<'src>>, span: Span },
    /// `object.field` — field access
    Field {
        object: Box<Expr<'src>>,
        field: &'src str,
        field_span: Span,
        span: Span,
    },
    /// `object.method(arg1, ...)` — method call
    MethodCall {
        object: Box<Expr<'src>>,
        method: &'src str,
        method_span: Span,
        args: Vec<Expr<'src>>,
        span: Span,
    },
    /// `expr?` — propagate operator (§12)
    Propagate { expr: Box<Expr<'src>>, span: Span },
    /// `expr as Type` — cast (§8)
    Cast { expr: Box<Expr<'src>>, ty: Box<Type<'src>>, span: Span },

    // --- Structured expressions ---
    Block(Box<Block<'src>>),
    /// `if cond { then } [else { else } | else if ...]`
    /// `else_branch` is `Expr::Block` or `Expr::If` for `else if` chains.
    If {
        condition: Box<Expr<'src>>,
        then_block: Box<Block<'src>>,
        else_branch: Option<Box<Expr<'src>>>,
        span: Span,
    },
    While { condition: Box<Expr<'src>>, body: Box<Block<'src>>, span: Span },
    Loop { body: Box<Block<'src>>, span: Span },
    /// `for binding in [&[mut]] iterable { body }`
    For {
        binding: &'src str,
        binding_span: Span,
        /// `None` = bare `for x in items`, `Some(Shared)` = `&items`, `Some(Mut)` = `&mut items`
        borrow: Option<BorrowKind>,
        iterable: Box<Expr<'src>>,
        body: Box<Block<'src>>,
        span: Span,
    },
    Match { subject: Box<Expr<'src>>, arms: Vec<MatchArm<'src>>, span: Span },
    /// `Name { field = expr, ... }` — struct literal (§10)
    StructLit {
        name: &'src str,
        name_span: Span,
        fields: Vec<StructFieldInit<'src>>,
        span: Span,
    },
}

impl Expr<'_> {
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(_, s) => *s,
            Expr::Ident(_, s) => *s,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Field { span, .. } => *span,
            Expr::MethodCall { span, .. } => *span,
            Expr::Propagate { span, .. } => *span,
            Expr::Cast { span, .. } => *span,
            Expr::Block(b) => b.span,
            Expr::If { span, .. } => *span,
            Expr::While { span, .. } => *span,
            Expr::Loop { span, .. } => *span,
            Expr::For { span, .. } => *span,
            Expr::Match { span, .. } => *span,
            Expr::StructLit { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldInit<'src> {
    pub name: &'src str,
    pub name_span: Span,
    pub value: Box<Expr<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm<'src> {
    pub pattern: Pattern<'src>,
    pub guard: Option<Box<Expr<'src>>>,
    pub body: Expr<'src>,
    pub span: Span,
}
