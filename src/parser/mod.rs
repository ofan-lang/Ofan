pub mod error;
pub use error::ParseError;

use crate::ast::Ast;
use crate::lexer::token::{Span, Token};

pub struct Parser {
    tokens: Vec<(Token, Span)>,
}

impl Parser {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self { tokens }
    }

    /// Parse the token stream into an AST.
    pub fn parse(&self) -> Result<Ast, ParseError> {
        // TODO: implement recursive-descent parser
        let _ = &self.tokens;
        Ok(Ast::default())
    }
}
