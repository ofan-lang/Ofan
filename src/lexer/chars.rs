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
        Some((escape_pos, '\\')) => super::decode_escape(
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
