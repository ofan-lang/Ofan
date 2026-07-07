use super::{LexError, Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

// Opening '"' is consumed by the caller before this is called.
pub(super) fn scan_string<'src>(
    src: &'src str,
    chars: &mut Peekable<CharIndices<'src>>,
    start: usize,
) -> Result<(Token<'src>, Span), LexError> {
    let content_start = chars.peek().map_or(start + 1, |&(p, _)| p);
    loop {
        match chars.next() {
            None => return Err(LexError::UnterminatedString { start }),
            Some((close_pos, '"')) => {
                let raw = &src[content_start..close_pos];
                return Ok((
                    Token::Str(raw),
                    Span {
                        start,
                        end: close_pos + 1,
                    },
                ));
            }
            Some((escape_pos, '\\')) => {
                // Validate escape; decoding is deferred to a later compiler phase.
                let _ = super::escapes::decode_escape(
                    chars,
                    escape_pos,
                    '"',
                    LexError::UnterminatedString { start },
                )?;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_string() {
        assert_eq!(
            lex(r#""hello""#).unwrap(),
            vec![Token::Str("hello"), Token::Eof]
        );
    }

    #[test]
    fn lex_string_valid_escape_returns_raw_slice() {
        assert_eq!(
            lex(r#""a\nb""#).unwrap(),
            vec![Token::Str(r"a\nb"), Token::Eof]
        );
    }

    #[test]
    fn lex_string_null_escape() {
        // \0 is a valid escape sequence (C-interop / FFI boundary use).
        assert!(lex("\"\\0\"").is_ok());
    }

    #[test]
    fn lex_string_invalid_escape() {
        assert!(matches!(
            lex(r#""\q""#),
            Err(LexError::InvalidEscape { ch: 'q', .. })
        ));
    }

    #[test]
    fn lex_string_reject_single_quote_escape() {
        // \' is not a valid string escape; only \" is the delimiter escape in strings.
        assert!(matches!(
            lex("\"\\'\""),
            Err(LexError::InvalidEscape { ch: '\'', .. })
        ));
    }

    #[test]
    fn lex_err_unterminated_string() {
        assert!(matches!(
            Lexer::new(r#""abc"#).lex(),
            Err(LexError::UnterminatedString { start: 0 })
        ));
    }
}
