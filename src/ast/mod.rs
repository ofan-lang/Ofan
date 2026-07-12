use crate::lexer::Span;

/// Top-level output of the parser: an ordered list of top-level items.
#[derive(Debug, Clone, PartialEq)]
pub struct Ast<'src> {
    pub items: Vec<Item<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item<'src> {
    Function(FunctionDef<'src>),
    // Struct, Enum, TypeAlias, ImplBlock — next PR
}

/// `fn name<params>(args) -> RetType { body }`
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef<'src> {
    pub name: &'src str,
    pub name_span: Span,
    /// Compile-time parameters: type params (`T`, `E`) and region tags (`r1`, `r2`).
    /// Role (type vs. region) is inferred from usage — decided in §7.
    pub generic_params: Vec<&'src str>,
    pub params: Vec<Param<'src>>,
    pub return_ty: Option<Type<'src>>,
    pub body: Block<'src>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param<'src> {
    pub name: &'src str,
    pub name_span: Span,
    pub ty: Type<'src>,
    pub span: Span,
}

/// A braced block: zero or more statements followed by an optional tail expression.
/// The tail expression (no trailing `;`) is the block's value.
#[derive(Debug, Clone, PartialEq)]
pub struct Block<'src> {
    pub stmts: Vec<Stmt<'src>>,
    pub tail: Option<Box<Expr<'src>>>,
    pub span: Span,
}

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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm<'src> {
    pub pattern: Pattern<'src>,
    pub guard: Option<Box<Expr<'src>>>,
    pub body: Expr<'src>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern<'src> {
    /// `_`
    Wildcard(Span),
    /// Integer, float, bool, char, or string literal
    Literal(Literal<'src>, Span),
    /// Bare identifier — type-checker resolves binding vs. unit-variant (§21)
    Name(&'src str, Span),
    /// `Name(sub_pattern, ...)` — tuple variant pattern
    Constructor {
        name: &'src str,
        name_span: Span,
        sub_patterns: Vec<Pattern<'src>>,
        span: Span,
    },
    /// `A | B` — or-pattern
    Or(Vec<Pattern<'src>>, Span),
}

impl Pattern<'_> {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(s) => *s,
            Pattern::Literal(_, s) => *s,
            Pattern::Name(_, s) => *s,
            Pattern::Constructor { span, .. } => *span,
            Pattern::Or(_, s) => *s,
        }
    }
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

/// Ofan type expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Type<'src> {
    /// Named type: primitives (`i32`, `f64`, `bool`, `str`), user-defined, or generic
    /// instantiation (`Option<T>`, `Checked<T, E>`).  Primitives are just `Named` with
    /// zero args; the type-checker resolves them.
    Named {
        name: &'src str,
        args: Vec<Type<'src>>,
        span: Span,
    },
    /// `&[mut] [region] InnerType` — borrow type (§7)
    Ref {
        mutable: bool,
        region: Option<RefRegion<'src>>,
        inner: Box<Type<'src>>,
        span: Span,
    },
    /// `Self` inside an impl block
    SelfTy(Span),
}

impl Type<'_> {
    pub fn span(&self) -> Span {
        match self {
            Type::Named { span, .. } => *span,
            Type::Ref { span, .. } => *span,
            Type::SelfTy(s) => *s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefRegion<'src> {
    Named(&'src str), // `r1`, `r2`, …
    Static,           // `static`
}
