use super::Token;

pub(super) fn lookup(text: &str) -> Option<Token<'_>> {
    match text {
        "fn" => Some(Token::Fn),
        "let" => Some(Token::Let),
        "mut" => Some(Token::Mut),
        "const" => Some(Token::Const),
        "if" => Some(Token::If),
        "else" => Some(Token::Else),
        "while" => Some(Token::While),
        "for" => Some(Token::For),
        "in" => Some(Token::In),
        "return" => Some(Token::Return),
        "break" => Some(Token::Break),
        "continue" => Some(Token::Continue),
        "true" => Some(Token::True),
        "false" => Some(Token::False),
        "struct" => Some(Token::Struct),
        "enum" => Some(Token::Enum),
        "pub" => Some(Token::Pub),
        "use" => Some(Token::Use),
        "as" => Some(Token::As),
        "using" => Some(Token::Using),
        "static" => Some(Token::Static),
        "unsafe" => Some(Token::Unsafe),
        _ => None,
    }
}
