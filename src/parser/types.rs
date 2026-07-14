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
                if name == "Self" {
                    return Ok(Type::SelfTy(name_span));
                }
                let args = self.parse_type_args_opt()?;
                let end = if args.is_empty() { name_span.end } else {
                    self.tokens[self.pos - 1].1.end
                };
                Ok(Type::Named { name, args, span: Span { start, end } })
            }
            // Lowercase `self` (Token::SelfKw) is a receiver value, never a type (§18).
            // Give a targeted error rather than the generic "expected a type" fallthrough.
            Token::SelfKw => Err(self.error_expected(
                "`Self` (capital)",
                Some("lowercase `self` is a receiver value, not a type — use `Self` to refer to the enclosing impl type (§18)"),
            )),
            _ => Err(self.error_expected("a type", Some("valid types: `i32`, `str`, `bool`, `&T`, `Option<T>`, `Checked<T, E>`, ..."))),
        }
    }

    /// Returns true if `tok` can begin a syntactically valid type (§7, §17, §18).
    /// Used by `try_parse_region_tag` so the "what starts a type" knowledge is not
    /// duplicated between the heuristic and `parse_type`'s own match.
    ///
    /// Note: `Token::SelfKw` (lowercase `self`) is excluded — it is never valid in
    /// type position (§18). `Self` (capital) lexes as `Token::Ident("Self")` and is
    /// covered by the `Ident` arm below.
    fn is_type_start_token(tok: &Token<'_>) -> bool {
        matches!(tok, Token::Ident(_) | Token::Amp)
    }

    /// Try to consume a region tag (`&r1 str`, `&static str`).
    fn try_parse_region_tag(&mut self) -> Option<RefRegion<'src>> {
        if matches!(self.peek(), Token::Static) {
            self.advance();
            return Some(RefRegion::Static);
        }
        if matches!(self.peek(), Token::Ident(_)) {
            let next_next = self.tokens.get(self.pos + 1).map(|(t, _)| t);
            if next_next.is_some_and(Self::is_type_start_token) {
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

    #[test]
    fn parse_type_self_ty() {
        let ty = parse_type("Self").unwrap();
        assert!(matches!(ty, Type::SelfTy(_)));
    }

    #[test]
    fn parse_type_ref_self_ty() {
        let ty = parse_type("&Self").unwrap();
        if let Type::Ref { mutable: false, region: None, inner, .. } = ty {
            assert!(matches!(*inner, Type::SelfTy(_)));
        } else { panic!("expected &Self"); }
    }

    #[test]
    fn parse_type_region_ref_self_ty() {
        let ty = parse_type("&r1 Self").unwrap();
        if let Type::Ref { region: Some(RefRegion::Named("r1")), inner, .. } = ty {
            assert!(matches!(*inner, Type::SelfTy(_)));
        } else { panic!("expected &r1 Self"); }
    }

    #[test]
    fn parse_type_self_kw_in_type_position_is_error() {
        let err = parse_type("self").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Self"), "error must mention `Self` (capital): {msg}");
        assert!(msg.contains("receiver"), "error must explain `self` is a receiver, not a type: {msg}");
        assert!(msg.contains("§18"), "error must cite §18: {msg}");
    }
}
