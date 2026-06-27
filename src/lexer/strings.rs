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
                let _ = super::decode_escape(
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
