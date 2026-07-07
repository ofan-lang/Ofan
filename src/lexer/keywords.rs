use super::{Span, Token};
use std::iter::Peekable;
use std::str::CharIndices;

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
        // §16 loop syntax — decided syntax
        "loop" => Some(Token::Loop),
        // §21 match / pattern matching — decided syntax
        "match" => Some(Token::Match),
        // Reserved ahead of syntax decisions (SYNTAX_SPEC.md §22).
        // Grammar for these constructs is undecided; words reserved so they
        // cannot be used as identifiers before that decision is made.
        "trait" => Some(Token::Trait),
        "mod" => Some(Token::Mod),
        _ => None,
    }
}

// iter arrives positioned AT the first char (not pre-consumed). The while loop
// consumes the first char on its first iteration, which correctly handles
// single-char identifiers (e.g. `x` at pos 0 → end = 0+1 = 1 → &src[0..1]).
pub(super) fn scan_identifier<'src>(
    src: &'src str,
    iter: &mut Peekable<CharIndices<'src>>,
    pos: usize,
) -> (Token<'src>, Span) {
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
    let tok = lookup(text).unwrap_or(Token::Ident(text));
    (tok, Span { start, end })
}

#[cfg(test)]
mod tests {
    use crate::lexer::{LexError, Lexer, Token};

    fn lex(src: &str) -> Result<Vec<Token<'_>>, LexError> {
        Lexer::new(src).lex().map(|ts| ts.into_iter().map(|(t, _)| t).collect())
    }

    #[test]
    fn lex_ident_vs_keyword() {
        assert_eq!(
            lex("fn fni").unwrap(),
            vec![Token::Fn, Token::Ident("fni"), Token::Eof]
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
    fn lex_unsafe_keyword() {
        assert_eq!(lex("unsafe").unwrap(), vec![Token::Unsafe, Token::Eof]);
    }

    #[test]
    fn lex_newly_reserved_keywords_decided_syntax() {
        // §16: loop syntax decided.
        assert_eq!(lex("loop").unwrap(), vec![Token::Loop, Token::Eof]);
        // §17: copy and move are decided modifiers, must not lex as Ident.
        assert_eq!(lex("copy").unwrap(), vec![Token::Copy, Token::Eof]);
        assert_eq!(lex("move").unwrap(), vec![Token::Move, Token::Eof]);
        // §18: self and impl are decided syntax, must not lex as Ident.
        assert_eq!(lex("self").unwrap(), vec![Token::SelfKw, Token::Eof]);
        assert_eq!(lex("impl").unwrap(), vec![Token::Impl, Token::Eof]);
        // §21: match syntax decided.
        assert_eq!(lex("match").unwrap(), vec![Token::Match, Token::Eof]);
    }

    #[test]
    fn lex_newly_reserved_keywords_future_syntax() {
        // §22 reservations: grammar undecided, words reserved so they cannot be
        // used as identifiers before a syntax decision is made.
        assert_eq!(lex("trait").unwrap(), vec![Token::Trait, Token::Eof]);
        assert_eq!(lex("mod").unwrap(), vec![Token::Mod, Token::Eof]);
    }

    #[test]
    fn lex_already_reserved_keywords_unchanged() {
        // Spot-check that previously-reserved words were not accidentally
        // removed or changed by this pass.
        assert_eq!(lex("while").unwrap(), vec![Token::While, Token::Eof]);
        assert_eq!(lex("for").unwrap(), vec![Token::For, Token::Eof]);
        assert_eq!(lex("in").unwrap(), vec![Token::In, Token::Eof]);
        assert_eq!(lex("enum").unwrap(), vec![Token::Enum, Token::Eof]);
        assert_eq!(lex("use").unwrap(), vec![Token::Use, Token::Eof]);
        assert_eq!(lex("if").unwrap(), vec![Token::If, Token::Eof]);
        assert_eq!(lex("else").unwrap(), vec![Token::Else, Token::Eof]);
        assert_eq!(lex("return").unwrap(), vec![Token::Return, Token::Eof]);
        assert_eq!(lex("true").unwrap(), vec![Token::True, Token::Eof]);
        assert_eq!(lex("false").unwrap(), vec![Token::False, Token::Eof]);
    }
}
