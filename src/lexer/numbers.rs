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
