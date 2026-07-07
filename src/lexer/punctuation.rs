use super::Token;

pub(super) fn lookup(ch: char) -> Option<Token<'static>> {
    match ch {
        '(' => Some(Token::LParen),
        ')' => Some(Token::RParen),
        '{' => Some(Token::LBrace),
        '}' => Some(Token::RBrace),
        '[' => Some(Token::LBracket),
        ']' => Some(Token::RBracket),
        ';' => Some(Token::Semicolon),
        ':' => Some(Token::Colon),
        ',' => Some(Token::Comma),
        '.' => Some(Token::Dot),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_punctuation_table() {
        assert_eq!(
            lex("()[]{};:,.").unwrap(),
            vec![
                Token::LParen,
                Token::RParen,
                Token::LBracket,
                Token::RBracket,
                Token::LBrace,
                Token::RBrace,
                Token::Semicolon,
                Token::Colon,
                Token::Comma,
                Token::Dot,
                Token::Eof,
            ]
        );
    }
}
