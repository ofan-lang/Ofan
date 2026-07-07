use super::{LexError, Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

fn check_no_ident_follows(
    src: &str,
    chars: &mut Peekable<CharIndices<'_>>,
    start: usize,
    end: usize,
) -> Result<(), LexError> {
    if let Some(&(_, ch)) = chars.peek() {
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Err(LexError::IdentAfterNumericLiteral {
                start,
                literal: src[start..end].to_string(),
                ch,
            });
        }
    }
    Ok(())
}

// First digit is consumed by the caller (passed as `first_digit`).
// `src` is needed only for the IdentAfterNumericLiteral error: the accumulated digit
// strings strip separators and drop the base prefix, so `src[start..end]` is the only
// way to reproduce the literal exactly as written.
pub(super) fn scan_number<'src>(
    src: &'src str,
    chars: &mut Peekable<CharIndices<'src>>,
    start: usize,
    first_digit: char,
) -> Result<(Token<'src>, Span), LexError> {
    let mut end = start + 1;

    let prefix_ch = if first_digit == '0' {
        chars.peek().and_then(|&(_, c)| {
            if c == 'x' || c == 'b' || c == 'o' {
                Some(c)
            } else {
                None
            }
        })
    } else {
        None
    };

    if let Some(prefix_ch) = prefix_ch {
        chars.next(); // consume base marker
        end = start + 2;

        let (is_valid_digit, radix): (fn(char) -> bool, u32) = match prefix_ch {
            'x' => (|c: char| c.is_ascii_hexdigit(), 16),
            'b' => (|c: char| c == '0' || c == '1', 2),
            'o' => (|c: char| matches!(c, '0'..='7'), 8),
            _ => unreachable!(),
        };

        let mut digits = String::new();
        let mut have_digit = false;
        while chars
            .peek()
            .is_some_and(|&(_, c)| is_valid_digit(c) || c == '_')
        {
            let (p, c) = chars.next().unwrap();
            end = p + 1;
            if c == '_' {
                if !have_digit || !chars.peek().is_some_and(|&(_, nc)| is_valid_digit(nc)) {
                    return Err(LexError::MisplacedDigitSeparator { byte: p });
                }
            } else {
                digits.push(c);
                have_digit = true;
            }
        }

        if digits.is_empty() {
            return Err(LexError::MissingDigitsAfterBase {
                start,
                marker: prefix_ch,
            });
        }
        let value =
            i64::from_str_radix(&digits, radix).map_err(|_| LexError::IntegerOverflow { start })?;
        check_no_ident_follows(src, chars, start, end)?;
        Ok((Token::Integer(value), Span { start, end }))
    } else {
        let mut int_digits = String::new();
        int_digits.push(first_digit);
        while chars
            .peek()
            .is_some_and(|&(_, c)| c.is_ascii_digit() || c == '_')
        {
            let (p, c) = chars.next().unwrap();
            end = p + 1;
            if c == '_' {
                if !chars.peek().is_some_and(|&(_, nc)| nc.is_ascii_digit()) {
                    return Err(LexError::MisplacedDigitSeparator { byte: p });
                }
            } else {
                int_digits.push(c);
            }
        }

        let is_float = chars.peek().is_some_and(|&(_, c)| c == '.') && {
            let mut tmp = chars.clone();
            tmp.next();
            tmp.peek().is_some_and(|&(_, c)| c.is_ascii_digit())
        };

        if is_float {
            chars.next(); // consume dot
            let mut frac_digits = String::new();
            while chars
                .peek()
                .is_some_and(|&(_, c)| c.is_ascii_digit() || c == '_')
            {
                let (p, c) = chars.next().unwrap();
                end = p + 1;
                if c == '_' {
                    if !chars.peek().is_some_and(|&(_, nc)| nc.is_ascii_digit()) {
                        return Err(LexError::MisplacedDigitSeparator { byte: p });
                    }
                } else {
                    frac_digits.push(c);
                }
            }
            let float_str = format!("{}.{}", int_digits, frac_digits);
            let value: f64 = float_str
                .parse()
                .map_err(|_| LexError::MalformedFloat { start })?;
            check_no_ident_follows(src, chars, start, end)?;
            Ok((Token::Float(value), Span { start, end }))
        } else {
            let value: i64 = int_digits
                .parse()
                .map_err(|_| LexError::IntegerOverflow { start })?;
            check_no_ident_follows(src, chars, start, end)?;
            Ok((Token::Integer(value), Span { start, end }))
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
    fn lex_integer() {
        assert_eq!(lex("42").unwrap(), vec![Token::Integer(42), Token::Eof]);
    }

    #[test]
    fn lex_float() {
        assert_eq!(lex("3.14").unwrap(), vec![Token::Float(3.14), Token::Eof]);
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
