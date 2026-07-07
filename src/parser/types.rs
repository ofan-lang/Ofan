use crate::ast::{RefRegion, Type};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(crate) fn parse_type(&mut self) -> Result<Type<'src>, ParseError> {
        let start = self.peek_span().start;
        match self.peek() {
            Token::Amp => {
                self.advance();
                let mutable = if matches!(self.peek(), Token::Mut) { self.advance(); true } else { false };
                // Optional region tag: heuristic — if next is Ident and next-next is also
                // a type-start token, treat the first ident as a region tag.
                let region = self.try_parse_region_tag();
                let inner = Box::new(self.parse_type()?);
                let end = inner.span().end;
                Ok(Type::Ref { mutable, region, inner, span: Span { start, end } })
            }
            Token::Ident(_) => {
                let (name, name_span) = self.eat_ident()?;
                // Capital `Self` spelled as an ident is the canonical type spelling (§18).
                // Lowercase `self` (Token::SelfKw) in type position is a parse error —
                // it is valid only as a value/receiver, not a type name.
                if name == "Self" {
                    return Ok(Type::SelfTy(name_span));
                }
                let args = self.parse_type_args_opt()?;
                let end = if args.is_empty() { name_span.end } else {
                    self.tokens[self.pos - 1].1.end
                };
                Ok(Type::Named { name, args, span: Span { start, end } })
            }
            _ => Err(self.error_expected("a type", Some("valid types: `i32`, `str`, `bool`, `&T`, `Option<T>`, `Checked<T, E>`, ..."))),
        }
    }

    /// Try to consume a region tag (`&r1 str`, `&static str`).
    fn try_parse_region_tag(&mut self) -> Option<RefRegion<'src>> {
        if matches!(self.peek(), Token::Static) {
            self.advance();
            return Some(RefRegion::Static);
        }
        if matches!(self.peek(), Token::Ident(_)) {
            let next_next = self.tokens.get(self.pos + 1).map(|(t, _)| t);
            let is_type_start = matches!(
                next_next,
                Some(Token::Ident(_) | Token::Amp | Token::SelfKw)
            );
            if is_type_start {
                if let Token::Ident(name) = *self.peek() {
                    self.advance();
                    return Some(RefRegion::Named(name));
                }
            }
        }
        None
    }

    /// Optional `<Type, Type, ...>` — returns empty vec if not present.
    fn parse_type_args_opt(&mut self) -> Result<Vec<Type<'src>>, ParseError> {
        if !matches!(self.peek(), Token::Lt) {
            return Ok(vec![]);
        }
        self.advance();
        let mut args = Vec::new();
        loop {
            if matches!(self.peek(), Token::Gt) {
                self.advance();
                break;
            }
            args.push(self.parse_type()?);
            match self.peek() {
                Token::Comma => { self.advance(); }
                Token::Gt => { self.advance(); break; }
                _ => return Err(self.error_expected("`,` or `>`", Some("add `,` to separate type arguments or `>` to close the list"))),
            }
        }
        Ok(args)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ast::{RefRegion, Type};
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn parse_type(src: &str) -> Result<Type<'_>, crate::parser::ParseError> {
        let tokens = Lexer::new(src).lex().expect("lex failed in test helper");
        Parser::new(tokens).parse_type()
    }

    #[test]
    fn parse_type_named() {
        let ty = parse_type("i32").unwrap();
        assert!(matches!(ty, Type::Named { name: "i32", args, .. } if args.is_empty()));
    }

    #[test]
    fn parse_type_generic() {
        let ty = parse_type("Option<i32>").unwrap();
        if let Type::Named { name: "Option", args, .. } = ty {
            assert_eq!(args.len(), 1);
        } else { panic!("expected Named"); }
    }

    #[test]
    fn parse_type_ref() {
        let ty = parse_type("&str").unwrap();
        assert!(matches!(ty, Type::Ref { mutable: false, .. }));
    }

    #[test]
    fn parse_type_ref_mut() {
        let ty = parse_type("&mut str").unwrap();
        assert!(matches!(ty, Type::Ref { mutable: true, .. }));
    }

    #[test]
    fn parse_type_static_ref() {
        let ty = parse_type("&static str").unwrap();
        if let Type::Ref { region: Some(RefRegion::Static), .. } = ty { }
        else { panic!("expected static ref"); }
    }

    #[test]
    fn parse_type_region_tag() {
        let ty = parse_type("&r1 str").unwrap();
        if let Type::Ref { region: Some(RefRegion::Named("r1")), .. } = ty { }
        else { panic!("expected region tag r1"); }
    }
}
