use super::LexError;
use std::iter::Peekable;
use std::str::CharIndices;

pub(super) fn decode_escape(
    chars: &mut Peekable<CharIndices<'_>>,
    escape_pos: usize,
    delimiter: char,
    eof_err: LexError,
) -> Result<char, LexError> {
    match chars.next() {
        Some((_, 'n')) => Ok('\n'),
        Some((_, 't')) => Ok('\t'),
        Some((_, 'r')) => Ok('\r'),
        Some((_, '\\')) => Ok('\\'),
        Some((_, '0')) => Ok('\0'),
        Some((_, c)) if c == delimiter => Ok(c),
        Some((_, ch)) => Err(LexError::InvalidEscape {
            byte: escape_pos,
            ch,
        }),
        None => Err(eof_err),
    }
}
