use super::{LexError, Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

// First '#' is consumed by the caller before this is called.
pub(super) fn scan_comment<'src>(
    src: &'src str,
    chars: &mut Peekable<CharIndices<'src>>,
    start: usize,
) -> Result<Option<(Token<'src>, Span)>, LexError> {
    let is_second_hash = chars.peek().is_some_and(|&(_, c)| c == '#');
    if is_second_hash {
        chars.next(); // consume second '#'
        let is_third_hash = chars.peek().is_some_and(|&(_, c)| c == '#');
        if is_third_hash {
            chars.next(); // consume third '#'
                          // Doc comment: scan until closing triple '#'.
            let content_start = chars.peek().map_or(src.len(), |&(p, _)| p);
            let mut content_end = content_start;
            loop {
                match chars.next() {
                    None => return Err(LexError::UnterminatedDocComment { start }),
                    Some((p, '#')) => {
                        if chars.peek().is_some_and(|&(_, c)| c == '#') {
                            chars.next();
                            if chars.peek().is_some_and(|&(_, c)| c == '#') {
                                chars.next();
                                break;
                            }
                        }
                        content_end = p + 1;
                    }
                    Some((p, c)) => {
                        content_end = p + c.len_utf8();
                    }
                }
            }
            let raw = &src[content_start..content_end];
            Ok(Some((
                Token::DocComment(raw),
                Span {
                    start,
                    end: content_end,
                },
            )))
        } else {
            // Block comment: scan until closing '##'.
            loop {
                match chars.next() {
                    None => return Err(LexError::UnterminatedBlockComment { start }),
                    Some((_, '#')) if chars.peek().is_some_and(|&(_, c)| c == '#') => {
                        chars.next();
                        break;
                    }
                    _ => {}
                }
            }
            Ok(None)
        }
    } else {
        // Line comment: scan to end of line.
        while chars.peek().is_some_and(|&(_, c)| c != '\n') {
            chars.next();
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_line_comment_skipped() {
        assert_eq!(
            lex("# comment\n42").unwrap(),
            vec![Token::Integer(42), Token::Eof]
        );
    }

    #[test]
    fn lex_block_comment_skipped() {
        assert_eq!(
            lex("## block\ncomment ##42").unwrap(),
            vec![Token::Integer(42), Token::Eof]
        );
    }

    #[test]
    fn lex_doc_comment_preserved() {
        let tokens = lex("### doc text ###").unwrap();
        assert!(matches!(&tokens[0], Token::DocComment(s) if s.trim() == "doc text"));
        assert_eq!(tokens[1], Token::Eof);
    }

    #[test]
    fn lex_err_unterminated_block_comment() {
        assert!(matches!(
            lex("## never closed"),
            Err(LexError::UnterminatedBlockComment { .. })
        ));
    }

    #[test]
    fn lex_err_unterminated_doc_comment() {
        assert!(matches!(
            lex("### never closed"),
            Err(LexError::UnterminatedDocComment { .. })
        ));
    }
}
