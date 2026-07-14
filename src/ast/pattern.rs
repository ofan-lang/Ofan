use crate::lexer::Span;
use super::Literal;

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
