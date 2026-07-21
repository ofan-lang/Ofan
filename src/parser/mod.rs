pub mod error;
pub use error::ParseError;

mod item;
mod types;
mod stmt;
mod expr;
mod control_flow;
mod pattern;

use crate::ast::Ast;
use crate::lexer::token::{Span, Token};
#[cfg(test)]
use crate::lexer::Lexer;

// ─── Parser struct & cursor ──────────────────────────────────────────────────

pub struct Parser<'src> {
    pub(crate) tokens: Vec<(Token<'src>, Span)>,
    pub(crate) pos: usize,
    pub(crate) no_struct_lit: bool,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<(Token<'src>, Span)>) -> Self {
        Parser { tokens, pos: 0, no_struct_lit: false }
    }

    // --- Cursor primitives ---

    pub(crate) fn peek(&self) -> &Token<'src> {
        &self.tokens[self.pos].0
    }

    pub(crate) fn peek_span(&self) -> Span {
        self.tokens[self.pos].1
    }

    pub(crate) fn advance(&mut self) -> (Token<'src>, Span) {
        let t = self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    /// Consume current token if it matches `expected` by discriminant. Returns its span.
    /// Suggestions for structural tokens are auto-derived from `structural_suggestion`.
    pub(crate) fn eat(&mut self, expected: &Token<'_>) -> Result<Span, ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            Ok(self.advance().1)
        } else {
            let suggestion = Self::structural_suggestion(expected);
            Err(self.error_expected(&format!("{expected}"), suggestion))
        }
    }

    /// Canonical suggestions for structural tokens (pillar 5).
    fn structural_suggestion(tok: &Token<'_>) -> Option<&'static str> {
        match tok {
            Token::Semicolon => Some("add `;` to end the statement"),
            Token::LBrace    => Some("add `{` to open the block body"),
            Token::RBrace    => Some("add `}` to close the block"),
            Token::LParen    => Some("add `(` to open the parameter list"),
            Token::RParen    => Some("add `)` to close the parenthesized expression"),
            Token::FatArrow  => Some("add `=>` after the pattern"),
            Token::Arrow     => Some("add `->` to specify the return type"),
            Token::Colon     => Some("add `:` to separate the name from its type"),
            Token::Equals    => Some("add `=` to begin the initializer"),
            Token::In        => Some("add `in` between the binding and the iterable"),
            Token::Comma     => Some("add `,` to separate items"),
            _ => None,
        }
    }

    /// Consume an `Ident` token, returning the source slice and its span.
    pub(crate) fn eat_ident(&mut self) -> Result<(&'src str, Span), ParseError> {
        match self.peek() {
            Token::Ident(_) => {
                let (tok, span) = self.advance();
                match tok {
                    Token::Ident(s) => Ok((s, span)),
                    _ => unreachable!(),
                }
            }
            _ => Err(self.error_expected("an identifier", Some("identifiers start with a letter or `_`, followed by letters, digits, or `_`"))),
        }
    }

    // --- Error helpers ---

    pub(crate) fn error_expected(&self, expected: &str, suggestion: Option<&str>) -> ParseError {
        let span = self.peek_span();
        let found = format!("{}", self.peek());
        let suggestion = suggestion.map(|s| s.to_string());
        if matches!(self.peek(), Token::Eof) {
            ParseError::UnexpectedEof { expected: expected.to_string(), suggestion }
        } else {
            ParseError::UnexpectedToken { span, found, expected: expected.to_string(), suggestion }
        }
    }

    // ─── Public entry point ──────────────────────────────────────────────────

    /// Parse the full token stream into an AST.
    pub fn parse(&mut self) -> Result<Ast<'src>, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek(), Token::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Ast { items })
    }
}

// ─── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
pub fn parse_expr(src: &str) -> Result<crate::ast::Expr<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_expr()
}

#[cfg(test)]
pub fn parse_stmt(src: &str) -> Result<crate::ast::Stmt<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_stmt()
}

#[cfg(test)]
pub fn parse_block(src: &str) -> Result<crate::ast::Block<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_block()
}

#[cfg(test)]
pub fn parse_fn(src: &str) -> Result<crate::ast::FunctionDef<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_function()
}

#[cfg(test)]
pub fn parse_type_str(src: &str) -> Result<crate::ast::Type<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_type()
}

#[cfg(test)]
pub fn parse_impl(src: &str) -> Result<crate::ast::ImplBlock<'_>, ParseError> {
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    Parser::new(tokens).parse_impl_block()
}

#[cfg(test)]
pub fn parse_struct(src: &str) -> Result<crate::ast::StructDef<'_>, ParseError> {
    use crate::ast::Item;
    let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
    let ast = Parser::new(tokens).parse()?;
    match ast.items.into_iter().next().expect("parse_struct: no items in source") {
        Item::Struct(def) => Ok(def),
        _ => panic!("parse_struct: first item is not a struct"),
    }
}
