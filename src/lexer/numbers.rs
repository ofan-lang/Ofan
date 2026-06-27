use std::iter::Peekable;
use std::str::CharIndices;
use super::{LexError, Span, Token};

// First digit is consumed by the caller (passed as `first_digit`).
pub(super) fn scan_number<'src>(
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
        while chars
            .peek()
            .is_some_and(|&(_, c)| is_valid_digit(c) || c == '_')
        {
            let (p, c) = chars.next().unwrap();
            end = p + 1;
            if c != '_' {
                digits.push(c);
            }
        }

        if digits.is_empty() {
            return Err(LexError::MissingDigitsAfterBase {
                start,
                marker: prefix_ch,
            });
        }
        let value = i64::from_str_radix(&digits, radix)
            .map_err(|_| LexError::IntegerOverflow { start })?;
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
            if c != '_' {
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
                if c != '_' {
                    frac_digits.push(c);
                }
            }
            let float_str = format!("{}.{}", int_digits, frac_digits);
            let value: f64 = float_str
                .parse()
                .map_err(|_| LexError::MalformedFloat { start })?;
            Ok((Token::Float(value), Span { start, end }))
        } else {
            let value: i64 = int_digits
                .parse()
                .map_err(|_| LexError::IntegerOverflow { start })?;
            Ok((Token::Integer(value), Span { start, end }))
        }
    }
}
