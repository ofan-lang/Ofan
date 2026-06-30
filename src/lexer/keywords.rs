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
        // §17 Copy/Move semantics — decided syntax, not yet in keyword table
        "copy" => Some(Token::Copy),
        "move" => Some(Token::Move),
        // §18 method receivers and impl blocks — decided syntax, not yet in keyword table
        "self" => Some(Token::SelfKw),
        "impl" => Some(Token::Impl),
        // Reserved ahead of syntax decisions (SYNTAX_SPEC.md §19).
        // Grammar for these constructs is undecided; words reserved so they
        // cannot be used as identifiers before that decision is made.
        "loop" => Some(Token::Loop),
        "match" => Some(Token::Match),
        "trait" => Some(Token::Trait),
        "mod" => Some(Token::Mod),
        _ => None,
    }
}
