pub mod token;
pub use token::{Span, Token};

pub struct Lexer<'src> {
    source: &'src str,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self { source }
    }

    /// Tokenize the source. Returns an empty program ending with `Eof`.
    pub fn lex(&self) -> Vec<(Token, Span)> {
        // TODO: implement tokenization
        vec![(
            Token::Eof,
            Span { start: self.source.len(), end: self.source.len() },
        )]
    }
}
