use super::{LexError, Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

// Opening '\'' is consumed by the caller before this is called.
pub(super) fn scan_char<'src>(
    chars: &mut Peekable<CharIndices<'src>>,
    start: usize,
) -> Result<(Token<'src>, Span), LexError> {
    let decoded: char = match chars.next() {
        None => return Err(LexError::UnterminatedChar { start }),
        Some((_, '\'')) => return Err(LexError::EmptyCharLiteral { start }),
        Some((escape_pos, '\\')) => super::escapes::decode_escape(
            chars,
            escape_pos,
            '\'',
            LexError::UnterminatedChar { start },
        )?,
        Some((_, c)) => c,
    };
    match chars.next() {
        Some((close_pos, '\'')) => Ok((
            Token::Char(decoded),
            Span {
                start,
                end: close_pos + 1,
            },
        )),
        Some(_) => Err(LexError::MultiCharLiteral { start }),
        None => Err(LexError::UnterminatedChar { start }),
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_char_literal() {
        assert_eq!(lex("'a'").unwrap(), vec![Token::Char('a'), Token::Eof]);
    }

    #[test]
    fn lex_char_escape() {
        assert_eq!(lex(r"'\n'").unwrap(), vec![Token::Char('\n'), Token::Eof]);
        assert_eq!(lex(r"'\''").unwrap(), vec![Token::Char('\''), Token::Eof]);
        assert_eq!(lex(r"'\0'").unwrap(), vec![Token::Char('\0'), Token::Eof]);
    }

    #[test]
    fn lex_char_empty_error() {
        assert!(matches!(lex("''"), Err(LexError::EmptyCharLiteral { .. })));
    }

    #[test]
    fn lex_char_multi_error() {
        assert!(matches!(
            lex("'ab'"),
            Err(LexError::MultiCharLiteral { .. })
        ));
    }

    #[test]
    fn lex_char_reject_double_quote_escape() {
        // \" is not a valid char escape; only \' is the delimiter escape in chars.
        assert!(matches!(
            lex(r#"'\"'"#),
            Err(LexError::InvalidEscape { ch: '"', .. })
        ));
    }
}
