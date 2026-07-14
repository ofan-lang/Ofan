use crate::lexer::Span;
use super::{Block, Type};

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
    pub consuming: bool,
    pub span: Span,
}
