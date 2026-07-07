pub mod error;
pub mod token;
pub use error::LexError;
pub use token::{Span, Token};

mod chars;
mod comments;
mod escapes;
mod keywords;
mod numbers;
mod operators;
mod punctuation;
mod strings;

pub struct Lexer<'src> {
    source: &'src str,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self { source }
    }

    pub fn lex(&self) -> Result<Vec<(Token<'src>, Span)>, LexError> {
        let mut tokens: Vec<(Token<'src>, Span)> = Vec::new();
        let src = self.source;
        let mut iter = src.char_indices().peekable();

        loop {
            // Skip whitespace.
            while iter.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
                iter.next();
            }

            let Some(&(pos, ch)) = iter.peek() else {
                tokens.push((
                    Token::Eof,
                    Span {
                        start: src.len(),
                        end: src.len(),
                    },
                ));
                break;
            };

            match ch {
                '#' => {
                    iter.next();
                    if let Some(tok) = comments::scan_comment(src, &mut iter, pos)? {
                        tokens.push(tok);
                    }
                }

                '"' => {
                    iter.next();
                    tokens.push(strings::scan_string(src, &mut iter, pos)?);
                }

                '0'..='9' => {
                    iter.next();
                    tokens.push(numbers::scan_number(src, &mut iter, pos, ch)?);
                }

                // iter arrives at the first char; scan_identifier's loop consumes it.
                'a'..='z' | 'A'..='Z' | '_' => {
                    tokens.push(keywords::scan_identifier(src, &mut iter, pos));
                }

                '\'' => {
                    iter.next();
                    tokens.push(chars::scan_char(&mut iter, pos)?);
                }

                '=' | '!' | '<' | '>' | '&' | '|' | '-' | '+' | '*' | '/' | '%' | '?' | '^'
                | '~' => {
                    iter.next();
                    tokens.push(operators::scan_operator(ch, &mut iter, pos));
                }

                _ => {
                    iter.next();
                    if let Some(tok) = punctuation::lookup(ch) {
                        push1(&mut tokens, tok, pos);
                    } else {
                        return Err(LexError::UnrecognizedCharacter { byte: pos, ch });
                    }
                }
            }
        }

        Ok(tokens)
    }
}

fn push1<'src>(tokens: &mut Vec<(Token<'src>, Span)>, tok: Token<'src>, pos: usize) {
    tokens.push((
        tok,
        Span {
            start: pos,
            end: pos + 1,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src)
            .lex()
            .map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_empty() {
        assert_eq!(lex("").unwrap(), vec![Token::Eof]);
    }

    #[test]
    fn lex_simple_tokens() {
        assert_eq!(
            lex("fn main() {}").unwrap(),
            vec![
                Token::Fn,
                Token::Ident("main"),
                Token::LParen,
                Token::RParen,
                Token::LBrace,
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lex_err_unrecognized_char() {
        assert!(matches!(
            Lexer::new("@").lex(),
            Err(LexError::UnrecognizedCharacter { byte: 0, ch: '@' })
        ));
    }
}
