use std::iter::Peekable;
use std::str::CharIndices;
use super::{LexError, Span, Token};

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
                Span { start, end: content_end },
            )))
        } else {
            // Block comment: scan until closing '##'.
            loop {
                match chars.next() {
                    None => return Err(LexError::UnterminatedBlockComment { start }),
                    Some((_, '#'))
                        if chars.peek().is_some_and(|&(_, c)| c == '#') =>
                    {
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
