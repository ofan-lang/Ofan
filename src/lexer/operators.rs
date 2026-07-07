use super::{Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

// Caller pre-consumes the operator char via iter.next() before calling.
// iter arrives positioned AFTER ch.
pub(super) fn scan_operator(
    ch: char,
    iter: &mut Peekable<CharIndices<'_>>,
    pos: usize,
) -> (Token<'static>, Span) {
    match ch {
        '=' => match iter.peek() {
            Some(&(end_pos, '=')) => {
                iter.next();
                (Token::EqEq, Span { start: pos, end: end_pos + 1 })
            }
            Some(&(end_pos, '>')) => {
                iter.next();
                (Token::FatArrow, Span { start: pos, end: end_pos + 1 })
            }
            _ => (Token::Equals, Span { start: pos, end: pos + 1 }),
        },
        '!' => {
            if iter.peek().is_some_and(|&(_, c)| c == '=') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::BangEq, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Bang, Span { start: pos, end: pos + 1 })
            }
        }
        '<' => match iter.peek() {
            Some(&(end_pos, '<')) => {
                iter.next();
                (Token::Shl, Span { start: pos, end: end_pos + 1 })
            }
            Some(&(end_pos, '=')) => {
                iter.next();
                (Token::LtEq, Span { start: pos, end: end_pos + 1 })
            }
            _ => (Token::Lt, Span { start: pos, end: pos + 1 }),
        },
        '>' => match iter.peek() {
            Some(&(end_pos, '>')) => {
                iter.next();
                (Token::Shr, Span { start: pos, end: end_pos + 1 })
            }
            Some(&(end_pos, '=')) => {
                iter.next();
                (Token::GtEq, Span { start: pos, end: end_pos + 1 })
            }
            _ => (Token::Gt, Span { start: pos, end: pos + 1 }),
        },
        '&' => {
            if iter.peek().is_some_and(|&(_, c)| c == '&') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::AmpAmp, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Amp, Span { start: pos, end: pos + 1 })
            }
        }
        '|' => {
            if iter.peek().is_some_and(|&(_, c)| c == '|') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::PipePipe, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Pipe, Span { start: pos, end: pos + 1 })
            }
        }
        '-' => match iter.peek() {
            Some(&(end_pos, '>')) => {
                iter.next();
                (Token::Arrow, Span { start: pos, end: end_pos + 1 })
            }
            Some(&(end_pos, '=')) => {
                iter.next();
                (Token::MinusEq, Span { start: pos, end: end_pos + 1 })
            }
            _ => (Token::Minus, Span { start: pos, end: pos + 1 }),
        },
        '+' => {
            if iter.peek().is_some_and(|&(_, c)| c == '=') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::PlusEq, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Plus, Span { start: pos, end: pos + 1 })
            }
        }
        '*' => {
            if iter.peek().is_some_and(|&(_, c)| c == '=') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::StarEq, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Star, Span { start: pos, end: pos + 1 })
            }
        }
        '/' => {
            if iter.peek().is_some_and(|&(_, c)| c == '=') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::SlashEq, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Slash, Span { start: pos, end: pos + 1 })
            }
        }
        '%' => {
            if iter.peek().is_some_and(|&(_, c)| c == '=') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::PercentEq, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Percent, Span { start: pos, end: pos + 1 })
            }
        }
        '?' => {
            if iter.peek().is_some_and(|&(_, c)| c == ':') {
                let (end_pos, _) = iter.next().unwrap();
                (Token::QuestionColon, Span { start: pos, end: end_pos + 1 })
            } else {
                (Token::Question, Span { start: pos, end: pos + 1 })
            }
        }
        '^' => (Token::Caret, Span { start: pos, end: pos + 1 }),
        '~' => (Token::Tilde, Span { start: pos, end: pos + 1 }),
        _ => unreachable!("scan_operator called on non-operator char: {ch:?}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_operators() {
        assert_eq!(
            lex("== != <= >= && ||").unwrap(),
            vec![
                Token::EqEq,
                Token::BangEq,
                Token::LtEq,
                Token::GtEq,
                Token::AmpAmp,
                Token::PipePipe,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lex_arrow() {
        assert_eq!(lex("->").unwrap(), vec![Token::Arrow, Token::Eof]);
    }

    #[test]
    fn lex_fat_arrow() {
        // §21 match arm separator
        assert_eq!(lex("=>").unwrap(), vec![Token::FatArrow, Token::Eof]);
    }

    #[test]
    fn lex_fat_arrow_does_not_disturb_adjacent_tokens() {
        // `=` alone, `==`, and `>=` must be unaffected by the `=>` scanning path.
        assert_eq!(lex("=").unwrap(), vec![Token::Equals, Token::Eof]);
        assert_eq!(lex("==").unwrap(), vec![Token::EqEq, Token::Eof]);
        assert_eq!(lex(">=").unwrap(), vec![Token::GtEq, Token::Eof]);
        // sequence: `= >` (with space) must produce two separate tokens
        assert_eq!(
            lex("= >").unwrap(),
            vec![Token::Equals, Token::Gt, Token::Eof]
        );
    }

    #[test]
    fn lex_compound_assignment() {
        assert_eq!(
            lex("+= -= *= /= %=").unwrap(),
            vec![
                Token::PlusEq,
                Token::MinusEq,
                Token::StarEq,
                Token::SlashEq,
                Token::PercentEq,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lex_bitwise() {
        assert_eq!(
            lex("& | ^ ~ << >>").unwrap(),
            vec![
                Token::Amp,
                Token::Pipe,
                Token::Caret,
                Token::Tilde,
                Token::Shl,
                Token::Shr,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lex_question() {
        assert_eq!(
            lex("? ?:").unwrap(),
            vec![Token::Question, Token::QuestionColon, Token::Eof]
        );
    }
}
