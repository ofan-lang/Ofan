use thiserror::Error;

#[derive(Debug, Error)]
pub enum LexError {
    #[error(
        "unrecognized character '{ch}' at byte {byte} — remove it or check for encoding issues"
    )]
    UnrecognizedCharacter { byte: usize, ch: char },

    #[error("unterminated string literal starting at byte {start} — add a closing '\"'")]
    UnterminatedString { start: usize },

    #[error("integer literal at byte {start} is too large for i64 — use a smaller value (i64 max: 9223372036854775807)")]
    IntegerOverflow { start: usize },

    #[error("malformed float literal at byte {start} — expected digits on both sides of '.'")]
    MalformedFloat { start: usize },

    #[error("unknown escape sequence `\\{ch}` at byte {byte} — valid escapes: `\\\"` `\\\\` `\\n` `\\t` `\\r` `\\0`")]
    InvalidEscape { byte: usize, ch: char },

    #[error("unterminated character literal starting at byte {start} — add a closing `'`")]
    UnterminatedChar { start: usize },

    #[error("empty character literal at byte {start} — a char literal must contain exactly one character")]
    EmptyCharLiteral { start: usize },

    #[error("character literal at byte {start} contains more than one character — use a string literal `\"..\"` for multiple characters")]
    MultiCharLiteral { start: usize },

    #[error("unterminated block comment starting at byte {start} — add a closing `##`")]
    UnterminatedBlockComment { start: usize },

    #[error("unterminated doc comment starting at byte {start} — add a closing `###`")]
    UnterminatedDocComment { start: usize },

    #[error("missing digits after `0{marker}` prefix at byte {start} — add at least one valid digit (e.g. `0{marker}1`)")]
    MissingDigitsAfterBase { start: usize, marker: char },

    #[error("misplaced `_` in numeric literal at byte {byte} — digit separators are valid only between two digits (e.g. `1_000`), not at the start, end, or doubled")]
    MisplacedDigitSeparator { byte: usize },
}
