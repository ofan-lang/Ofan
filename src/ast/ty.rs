use crate::lexer::Span;

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
