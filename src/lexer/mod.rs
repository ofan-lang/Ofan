pub mod error;
pub mod token;
pub use error::LexError;
pub use token::{Span, Token};

mod chars;
mod comments;
mod keywords;
mod numbers;
mod strings;

use std::iter::Peekable;
use std::str::CharIndices;

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

                '/' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::SlashEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Slash, pos);
                    }
                }

                // String literals: validate escape sequences; return raw source slice.
                // Escape decoding is a later compiler phase.
                '"' => {
                    iter.next();
                    tokens.push(strings::scan_string(src, &mut iter, pos)?);
                }

                '0'..='9' => {
                    iter.next();
                    tokens.push(numbers::scan_number(src, &mut iter, pos, ch)?);
                }

                'a'..='z' | 'A'..='Z' | '_' => {
                    let start = pos;
                    let mut end = pos;
                    while iter
                        .peek()
                        .is_some_and(|&(_, c)| c.is_ascii_alphanumeric() || c == '_')
                    {
                        let (p, c) = iter.next().unwrap();
                        end = p + c.len_utf8();
                    }
                    let text = &src[start..end];
                    let tok = keywords::lookup(text).unwrap_or(Token::Ident(text));
                    tokens.push((tok, Span { start, end }));
                }

                '\'' => {
                    iter.next();
                    tokens.push(chars::scan_char(&mut iter, pos)?);
                }

                '=' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::EqEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Equals, pos);
                    }
                }
                '!' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::BangEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Bang, pos);
                    }
                }
                '<' => {
                    iter.next();
                    match iter.peek() {
                        Some(&(end_pos, '<')) => {
                            iter.next();
                            push2(&mut tokens, Token::Shl, pos, end_pos);
                        }
                        Some(&(end_pos, '=')) => {
                            iter.next();
                            push2(&mut tokens, Token::LtEq, pos, end_pos);
                        }
                        _ => push1(&mut tokens, Token::Lt, pos),
                    }
                }
                '>' => {
                    iter.next();
                    match iter.peek() {
                        Some(&(end_pos, '>')) => {
                            iter.next();
                            push2(&mut tokens, Token::Shr, pos, end_pos);
                        }
                        Some(&(end_pos, '=')) => {
                            iter.next();
                            push2(&mut tokens, Token::GtEq, pos, end_pos);
                        }
                        _ => push1(&mut tokens, Token::Gt, pos),
                    }
                }
                '&' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '&') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::AmpAmp, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Amp, pos);
                    }
                }
                '|' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '|') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::PipePipe, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Pipe, pos);
                    }
                }
                '-' => {
                    iter.next();
                    match iter.peek() {
                        Some(&(end_pos, '>')) => {
                            iter.next();
                            push2(&mut tokens, Token::Arrow, pos, end_pos);
                        }
                        Some(&(end_pos, '=')) => {
                            iter.next();
                            push2(&mut tokens, Token::MinusEq, pos, end_pos);
                        }
                        _ => push1(&mut tokens, Token::Minus, pos),
                    }
                }
                '+' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::PlusEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Plus, pos);
                    }
                }
                '*' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::StarEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Star, pos);
                    }
                }
                '%' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == '=') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::PercentEq, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Percent, pos);
                    }
                }
                '?' => {
                    iter.next();
                    if iter.peek().is_some_and(|&(_, c)| c == ':') {
                        let (end_pos, _) = iter.next().unwrap();
                        push2(&mut tokens, Token::QuestionColon, pos, end_pos);
                    } else {
                        push1(&mut tokens, Token::Question, pos);
                    }
                }
                '^' => {
                    iter.next();
                    push1(&mut tokens, Token::Caret, pos);
                }
                '~' => {
                    iter.next();
                    push1(&mut tokens, Token::Tilde, pos);
                }
                '(' => {
                    iter.next();
                    push1(&mut tokens, Token::LParen, pos);
                }
                ')' => {
                    iter.next();
                    push1(&mut tokens, Token::RParen, pos);
                }
                '{' => {
                    iter.next();
                    push1(&mut tokens, Token::LBrace, pos);
                }
                '}' => {
                    iter.next();
                    push1(&mut tokens, Token::RBrace, pos);
                }
                '[' => {
                    iter.next();
                    push1(&mut tokens, Token::LBracket, pos);
                }
                ']' => {
                    iter.next();
                    push1(&mut tokens, Token::RBracket, pos);
                }
                ';' => {
                    iter.next();
                    push1(&mut tokens, Token::Semicolon, pos);
                }
                ':' => {
                    iter.next();
                    push1(&mut tokens, Token::Colon, pos);
                }
                ',' => {
                    iter.next();
                    push1(&mut tokens, Token::Comma, pos);
                }
                '.' => {
                    iter.next();
                    push1(&mut tokens, Token::Dot, pos);
                }

                _ => {
                    return Err(LexError::UnrecognizedCharacter { byte: pos, ch });
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

fn push2<'src>(
    tokens: &mut Vec<(Token<'src>, Span)>,
    tok: Token<'src>,
    pos: usize,
    end_pos: usize,
) {
    tokens.push((
        tok,
        Span {
            start: pos,
            end: end_pos + 1,
        },
    ));
}

// Shared escape-sequence validator used by strings.rs and chars.rs.
// Returns the decoded char so chars.rs can use it; strings.rs discards the value.
fn decode_escape(
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
    fn lex_ident_vs_keyword() {
        assert_eq!(
            lex("fn fni").unwrap(),
            vec![Token::Fn, Token::Ident("fni"), Token::Eof]
        );
    }

    #[test]
    fn lex_integer() {
        assert_eq!(lex("42").unwrap(), vec![Token::Integer(42), Token::Eof]);
    }

    #[test]
    fn lex_float() {
        assert_eq!(lex("3.14").unwrap(), vec![Token::Float(3.14), Token::Eof]);
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

    #[test]
    fn lex_keywords_as_using_static() {
        assert_eq!(
            lex("as using static").unwrap(),
            vec![Token::As, Token::Using, Token::Static, Token::Eof]
        );
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
    fn lex_unsafe_keyword() {
        assert_eq!(lex("unsafe").unwrap(), vec![Token::Unsafe, Token::Eof]);
    }

    #[test]
    fn lex_hex_literal() {
        assert_eq!(lex("0xFF").unwrap(), vec![Token::Integer(255), Token::Eof]);
        assert_eq!(
            lex("0x4002_0014").unwrap(),
            vec![Token::Integer(0x4002_0014), Token::Eof]
        );
    }

    #[test]
    fn lex_binary_literal() {
        assert_eq!(lex("0b1010").unwrap(), vec![Token::Integer(10), Token::Eof]);
        assert_eq!(
            lex("0b1010_1100").unwrap(),
            vec![Token::Integer(0b1010_1100), Token::Eof]
        );
    }

    #[test]
    fn lex_octal_literal() {
        assert_eq!(lex("0o17").unwrap(), vec![Token::Integer(15), Token::Eof]);
        assert_eq!(
            lex("0o755").unwrap(),
            vec![Token::Integer(0o755), Token::Eof]
        );
    }

    #[test]
    fn lex_digit_separator() {
        assert_eq!(
            lex("1_000_000").unwrap(),
            vec![Token::Integer(1_000_000), Token::Eof]
        );
        assert_eq!(lex("3.14").unwrap(), vec![Token::Float(3.14), Token::Eof]);
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
    fn lex_err_unterminated_string() {
        assert!(matches!(
            Lexer::new(r#""abc"#).lex(),
            Err(LexError::UnterminatedString { start: 0 })
        ));
    }

    #[test]
    fn lex_err_unrecognized_char() {
        assert!(matches!(
            Lexer::new("@").lex(),
            Err(LexError::UnrecognizedCharacter { byte: 0, ch: '@' })
        ));
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

    #[test]
    fn lex_string_reject_single_quote_escape() {
        // \' is not a valid string escape; only \" is the delimiter escape in strings.
        assert!(matches!(
            lex("\"\\'\""),
            Err(LexError::InvalidEscape { ch: '\'', .. })
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

    #[test]
    fn lex_err_missing_digits_after_base() {
        assert!(matches!(
            lex("0x"),
            Err(LexError::MissingDigitsAfterBase { marker: 'x', .. })
        ));
        assert!(matches!(
            lex("0b"),
            Err(LexError::MissingDigitsAfterBase { marker: 'b', .. })
        ));
        assert!(matches!(
            lex("0o"),
            Err(LexError::MissingDigitsAfterBase { marker: 'o', .. })
        ));
    }

    #[test]
    fn lex_err_misplaced_digit_separator_decimal() {
        // trailing
        assert!(matches!(
            lex("1000_"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
        // doubled
        assert!(matches!(
            lex("1__000"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
    }

    #[test]
    fn lex_err_misplaced_digit_separator_hex() {
        // leading after prefix
        assert!(matches!(
            lex("0x_FF"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
        // trailing
        assert!(matches!(
            lex("0xFF_"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
        // doubled
        assert!(matches!(
            lex("0x1__2"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
    }

    #[test]
    fn lex_err_misplaced_digit_separator_float() {
        // trailing in frac
        assert!(matches!(
            lex("1.5_"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
        // doubled in frac
        assert!(matches!(
            lex("1.5__3"),
            Err(LexError::MisplacedDigitSeparator { .. })
        ));
    }

    #[test]
    fn lex_digit_separator_valid_positions() {
        // valid: between digits in decimal, hex, binary, octal, and float
        assert!(matches!(lex("1_000"), Ok(_)));
        assert!(matches!(lex("0xFF_00"), Ok(_)));
        assert!(matches!(lex("0b1010_0101"), Ok(_)));
        assert!(matches!(lex("0o17_77"), Ok(_)));
        assert!(matches!(lex("3.141_592"), Ok(_)));
    }

    // ── IdentAfterNumericLiteral ──────────────────────────────────────────────

    #[test]
    fn lex_err_ident_after_decimal_integer() {
        assert!(matches!(
            lex("1abc"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'a' })
                if literal == "1"
        ));
        assert!(matches!(
            lex("42px"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'p' })
                if literal == "42"
        ));
        assert!(matches!(
            lex("1_000abc"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'a' })
                if literal == "1_000"
        ));
    }

    #[test]
    fn lex_err_ident_after_float() {
        assert!(matches!(
            lex("1.5abc"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'a' })
                if literal == "1.5"
        ));
    }

    #[test]
    fn lex_err_ident_after_hex() {
        assert!(matches!(
            lex("0x1fg"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'g' })
                if literal == "0x1f"
        ));
        assert!(matches!(
            lex("0xFEEDgabe"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'g' })
                if literal == "0xFEED"
        ));
    }

    #[test]
    fn lex_err_ident_after_binary() {
        assert!(matches!(
            lex("0b101z"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'z' })
                if literal == "0b101"
        ));
    }

    #[test]
    fn lex_err_ident_after_octal() {
        assert!(matches!(
            lex("0o71abc"),
            Err(LexError::IdentAfterNumericLiteral { start: 0, ref literal, ch: 'a' })
                if literal == "0o71"
        ));
    }

    #[test]
    fn lex_misplaced_separator_takes_precedence_over_ident_after_literal() {
        // 1_abc: the scan loop consumes '_', checks next char ('a') as digit lookahead,
        // fails, and returns MisplacedDigitSeparator before reaching the success exit
        // where IdentAfterNumericLiteral would be checked. First-problem-encountered,
        // not a priority ranking.
        assert!(matches!(
            lex("1_abc"),
            Err(LexError::MisplacedDigitSeparator { byte: 1 })
        ));
    }

    #[test]
    fn lex_numeric_followed_by_operator_or_whitespace_still_ok() {
        // Operator or whitespace after a literal must not trigger IdentAfterNumericLiteral.
        assert!(matches!(lex("42+1"), Ok(_)));
        assert!(matches!(lex("42 abc"), Ok(_)));
        assert!(matches!(lex("0xFF;"), Ok(_)));
        assert!(matches!(lex("1.5,"), Ok(_)));
    }
}
