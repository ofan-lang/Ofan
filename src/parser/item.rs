use crate::ast::{FunctionDef, Item, Param, Type};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(super) fn parse_item(&mut self) -> Result<Item<'src>, ParseError> {
        match self.peek() {
            Token::Fn => Ok(Item::Function(self.parse_function()?)),
            _ => Err(self.error_expected("`fn`", Some("only `fn` declarations are allowed at the top level"))),
        }
    }

    /// `fn name[<T, r1, ...>](params) [-> RetType] { body }`
    pub(super) fn parse_function(&mut self) -> Result<FunctionDef<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Fn)?;
        let (name, name_span) = self.eat_ident()?;
        let generic_params = self.parse_generic_params_opt()?;
        self.eat(&Token::LParen)?;
        let params = self.parse_params()?;
        self.eat(&Token::RParen)?;
        let return_ty = if matches!(self.peek(), Token::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let end = body.span.end;
        Ok(FunctionDef { name, name_span, generic_params, params, return_ty, body, span: Span { start, end } })
    }

    /// Optional `<T, r1, E, ...>` — returns empty vec if not present
    fn parse_generic_params_opt(&mut self) -> Result<Vec<&'src str>, ParseError> {
        if !matches!(self.peek(), Token::Lt) {
            return Ok(vec![]);
        }
        self.advance();
        let mut params = Vec::new();
        loop {
            if matches!(self.peek(), Token::Gt) {
                self.advance();
                break;
            }
            let (name, _) = self.eat_ident()?;
            params.push(name);
            match self.peek() {
                Token::Comma => { self.advance(); }
                Token::Gt => { self.advance(); break; }
                _ => return Err(self.error_expected("`,` or `>`", Some("add `,` to separate generic parameters or `>` to close the list"))),
            }
        }
        Ok(params)
    }

    /// Comma-separated parameter list (may be empty)
    fn parse_params(&mut self) -> Result<Vec<Param<'src>>, ParseError> {
        let mut params = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            return Ok(params);
        }
        loop {
            let start = self.peek_span().start;

            // Handle `self`, `&self`, `&mut self` receiver forms (§18)
            if matches!(self.peek(), Token::SelfKw) {
                let (_, self_span) = self.advance();
                params.push(Param {
                    name: "self",
                    name_span: self_span,
                    ty: Type::Named { name: "Self", args: vec![], span: self_span },
                    span: self_span,
                });
            } else if matches!(self.peek(), Token::Amp) {
                let amp_span = self.advance().1;
                let mutable = if matches!(self.peek(), Token::Mut) { self.advance(); true } else { false };
                if !matches!(self.peek(), Token::SelfKw) {
                    return Err(self.error_expected("`self`", Some("only `self`, `&self`, or `&mut self` are valid receiver forms")));
                }
                let (_, self_span) = self.advance();
                let end = self_span.end;
                params.push(Param {
                    name: "self",
                    name_span: self_span,
                    ty: Type::Ref {
                        mutable,
                        region: None,
                        inner: Box::new(Type::SelfTy(self_span)),
                        span: Span { start: amp_span.start, end },
                    },
                    span: Span { start: amp_span.start, end },
                });
            } else {
                let (name, name_span) = self.eat_ident()?;
                self.eat(&Token::Colon)?;
                let ty = self.parse_type()?;
                let end = ty.span().end;
                params.push(Param { name, name_span, ty, span: Span { start, end } });
            }

            match self.peek() {
                Token::Comma => { self.advance(); }
                Token::RParen => break,
                _ => return Err(self.error_expected("`,` or `)`", Some("add `,` to separate parameters or `)` to close the parameter list"))),
            }
        }
        Ok(params)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::parser::{parse_fn, Parser};

    // --- Function declarations ---

    #[test]
    fn parse_fn_no_params() {
        let f = parse_fn("fn hello() { }").unwrap();
        assert_eq!(f.name, "hello");
        assert!(f.params.is_empty());
        assert!(f.return_ty.is_none());
    }

    #[test]
    fn parse_fn_with_params_and_return() {
        let f = parse_fn("fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert!(f.return_ty.is_some());
    }

    #[test]
    fn parse_fn_generic() {
        let f = parse_fn("fn identity<T>(x: T) -> T { x }").unwrap();
        assert_eq!(f.generic_params, vec!["T"]);
        assert_eq!(f.params.len(), 1);
    }

    #[test]
    fn parse_fn_body_let_return() {
        let f = parse_fn("fn double(n: i32) -> i32 { let r: i32 = n * 2; return r; }").unwrap();
        assert_eq!(f.body.stmts.len(), 2);
        assert!(f.body.tail.is_none());
    }

    #[test]
    fn parse_fn_body_tail_expr() {
        let f = parse_fn("fn double(n: i32) -> i32 { n * 2 }").unwrap();
        assert_eq!(f.body.stmts.len(), 0);
        assert!(f.body.tail.is_some());
    }

    // --- Integration ---

    #[test]
    fn parse_factorial_function() {
        let src = "fn factorial(n: i32) -> i32 { match n { 0 => 1, n => n * factorial(n), } }";
        let f = parse_fn(src).unwrap();
        assert_eq!(f.name, "factorial");
        assert!(f.body.tail.is_some());
    }

    #[test]
    fn parse_loop_break_value() {
        let src = "fn find() -> i32 { let r: i32 = loop { break 42; }; r }";
        let f = parse_fn(src).unwrap();
        assert_eq!(f.body.stmts.len(), 1);
    }

    #[test]
    fn parse_full_ast() {
        let src = "fn hello() { } fn world() -> i32 { 42 }";
        let tokens = Lexer::new(src).lex().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        assert_eq!(ast.items.len(), 2);
    }
}
