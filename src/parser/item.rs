use crate::ast::{CopyMove, FunctionDef, ImplBlock, Item, Param, StructDef, StructField, Type};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(super) fn parse_item(&mut self) -> Result<Item<'src>, ParseError> {
        match self.peek() {
            Token::Fn     => Ok(Item::Function(self.parse_function()?)),
            Token::Impl   => Ok(Item::Impl(self.parse_impl_block()?)),
            Token::Struct => {
                let start = self.peek_span().start;
                Ok(Item::Struct(self.parse_struct_def(None, start)?))
            }
            Token::Copy => {
                let start = self.peek_span().start;
                self.advance();
                if !matches!(self.peek(), Token::Struct) {
                    return Err(self.error_expected(
                        "`struct`",
                        Some("`copy` at the top level is only valid as `copy struct` (§23)"),
                    ));
                }
                Ok(Item::Struct(self.parse_struct_def(Some(CopyMove::Copy), start)?))
            }
            Token::Move => {
                let start = self.peek_span().start;
                self.advance();
                if !matches!(self.peek(), Token::Struct) {
                    return Err(self.error_expected(
                        "`struct`",
                        Some("`move` at the top level is only valid as `move struct` (§23)"),
                    ));
                }
                Ok(Item::Struct(self.parse_struct_def(Some(CopyMove::Move), start)?))
            }
            _ => Err(self.error_expected(
                "`fn`, `impl`, or `struct`",
                Some("only `fn`, `impl`, and `struct` declarations are allowed at the top level"),
            )),
        }
    }

    /// `[copy|move] struct Name[<T, ...>] { field: Type, ... }`
    /// Caller has already consumed the `copy`/`move` modifier if present.
    /// `outer_start` is the byte offset of the modifier keyword (or `struct` keyword if none).
    fn parse_struct_def(
        &mut self,
        copy_move: Option<CopyMove>,
        outer_start: usize,
    ) -> Result<StructDef<'src>, ParseError> {
        self.eat(&Token::Struct)?;
        let (name, name_span) = self.eat_ident()?;
        let generic_params = self.parse_generic_params_opt()?;
        self.eat(&Token::LBrace)?;

        let mut fields = Vec::new();
        loop {
            match self.peek() {
                Token::RBrace => break,
                Token::Eof => return Err(self.error_expected(
                    "`}` or a field name",
                    Some("add `}` to close the struct body"),
                )),
                _ => {}
            }
            let field_start = self.peek_span().start;
            let (fname, fname_span) = self.eat_ident()?;
            self.eat(&Token::Colon)?;
            let ty = self.parse_type()?;
            let field_end = ty.span().end;
            fields.push(StructField {
                name: fname,
                name_span: fname_span,
                ty,
                span: Span { start: field_start, end: field_end },
            });
            match self.peek() {
                Token::Comma => { self.advance(); }
                Token::RBrace => break,
                _ => return Err(self.error_expected(
                    "`,` or `}`",
                    Some("add `,` to separate fields or `}` to close the struct"),
                )),
            }
        }

        let end = self.eat(&Token::RBrace)?.end;
        Ok(StructDef {
            name,
            name_span,
            copy_move,
            generic_params,
            fields,
            span: Span { start: outer_start, end },
        })
    }

    /// `impl TypeName { [fn ...] }`
    pub(super) fn parse_impl_block(&mut self) -> Result<ImplBlock<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Impl)?;
        let (type_name, type_name_span) = self.eat_ident()?;
        self.eat(&Token::LBrace)?;

        let mut methods = Vec::new();
        loop {
            match self.peek() {
                Token::RBrace => break,
                Token::Fn => methods.push(self.parse_function()?),
                Token::Eof => return Err(self.error_expected(
                    "`}` or `fn`",
                    Some("add `}` to close the impl block"),
                )),
                _ => return Err(self.error_expected(
                    "`fn`",
                    Some(
                        "impl blocks are declaration namespaces — only `fn` declarations \
                         are valid inside; variables, expressions, and statements are not \
                         permitted here (§22)",
                    ),
                )),
            }
        }

        let end = self.eat(&Token::RBrace)?.end;
        Ok(ImplBlock { type_name, type_name_span, methods, span: Span { start, end } })
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

            if matches!(self.peek(), Token::Move) {
                // `move self` — consuming receiver (§18)
                let move_span = self.advance().1;
                if !matches!(self.peek(), Token::SelfKw) {
                    return Err(self.error_expected(
                        "`self`",
                        Some("`move` in a parameter list is only valid as `move self` (consuming receiver — §18)"),
                    ));
                }
                let (_, self_span) = self.advance();
                params.push(Param {
                    name: "self",
                    name_span: self_span,
                    ty: Type::SelfTy(self_span),
                    consuming: true,
                    span: Span { start: move_span.start, end: self_span.end },
                });
            } else if matches!(self.peek(), Token::SelfKw) {
                // bare `self` — inferred-access receiver (§18)
                let (_, self_span) = self.advance();
                params.push(Param {
                    name: "self",
                    name_span: self_span,
                    ty: Type::SelfTy(self_span),
                    consuming: false,
                    span: self_span,
                });
            } else if matches!(self.peek(), Token::Amp) {
                // `&self` / `&mut self` do not exist in Ofan source (§18) — pillar-5 error.
                // Consume through the form so the error spans from `&`. Hand-rolling
                // ParseError::UnexpectedToken rather than using error_expected because we
                // want `found` to describe the consumed multi-token form, not the lookahead
                // token that the cursor happens to sit on after consuming `&`/`mut`/`self`.
                let amp_span = self.peek_span();
                self.advance(); // `&`
                let has_mut = if matches!(self.peek(), Token::Mut) { self.advance(); true } else { false };
                let has_self = if matches!(self.peek(), Token::SelfKw) { self.advance(); true } else { false };
                let form = match (has_mut, has_self) {
                    (false, true)  => "`&self`",
                    (true,  true)  => "`&mut self`",
                    (true,  false) => "`&mut`",
                    (false, false) => "`&`",
                };
                return Err(ParseError::UnexpectedToken {
                    span: amp_span,
                    found: form.to_string(),
                    expected: "`self` or `move self`".to_string(),
                    suggestion: Some(format!(
                        "{form} receiver form does not exist in Ofan — write bare `self` and \
                         let the compiler infer the borrow level from the body; use `move self` \
                         to force consuming ownership (§18)"
                    )),
                });
            } else {
                // regular named parameter
                let (name, name_span) = self.eat_ident()?;
                self.eat(&Token::Colon)?;
                let ty = self.parse_type()?;
                let end = ty.span().end;
                params.push(Param { name, name_span, ty, consuming: false, span: Span { start, end } });
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

    // --- Self receivers ---

    #[test]
    fn parse_fn_self_receiver() {
        use crate::ast::Type;
        let f = parse_fn("fn foo(self) -> i32 { 42 }").unwrap();
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.params[0].name, "self");
        assert!(!f.params[0].consuming);
        assert!(matches!(f.params[0].ty, Type::SelfTy(_)));
    }

    #[test]
    fn parse_fn_move_self_receiver() {
        use crate::ast::Type;
        let f = parse_fn("fn into_val(move self) -> i32 { 42 }").unwrap();
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.params[0].name, "self");
        assert!(f.params[0].consuming);
        assert!(matches!(f.params[0].ty, Type::SelfTy(_)));
    }

    #[test]
    fn parse_fn_ref_self_is_error() {
        let err = parse_fn("fn foo(&self) { }").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("§18"), "must cite §18: {msg}");
        assert!(msg.contains("infer"), "must mention inference: {msg}");
    }

    #[test]
    fn parse_fn_ref_mut_self_is_error() {
        let err = parse_fn("fn foo(&mut self) { }").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("§18"), "must cite §18: {msg}");
        assert!(msg.contains("infer"), "must mention inference: {msg}");
    }

    // --- impl blocks ---

    #[test]
    fn parse_impl_empty() {
        use crate::parser::parse_impl;
        let b = parse_impl("impl Foo { }").unwrap();
        assert_eq!(b.type_name, "Foo");
        assert!(b.methods.is_empty());
    }

    #[test]
    fn parse_impl_single_method() {
        use crate::ast::Type;
        use crate::parser::parse_impl;
        let b = parse_impl("impl Foo { fn bar(self) -> i32 { 42 } }").unwrap();
        assert_eq!(b.methods.len(), 1);
        let m = &b.methods[0];
        assert_eq!(m.name, "bar");
        assert_eq!(m.params.len(), 1);
        assert_eq!(m.params[0].name, "self");
        assert!(!m.params[0].consuming);
        assert!(matches!(m.params[0].ty, Type::SelfTy(_)));
    }

    #[test]
    fn parse_impl_move_self_method() {
        use crate::ast::Type;
        use crate::parser::parse_impl;
        let b = parse_impl("impl Foo { fn consume(move self) { } }").unwrap();
        assert_eq!(b.methods.len(), 1);
        assert!(b.methods[0].params[0].consuming);
        assert!(matches!(b.methods[0].params[0].ty, Type::SelfTy(_)));
    }

    #[test]
    fn parse_impl_associated_fn_self_return() {
        use crate::ast::Type;
        use crate::parser::parse_impl;
        // associated fn: no receiver; Self in return position
        let b = parse_impl("impl Foo { fn default() -> Self { 0 } }").unwrap();
        assert_eq!(b.methods.len(), 1);
        let m = &b.methods[0];
        assert_eq!(m.name, "default");
        assert!(m.params.is_empty());
        assert!(matches!(m.return_ty, Some(Type::SelfTy(_))));
    }

    #[test]
    fn parse_impl_mixed_method_and_assoc_fn() {
        use crate::ast::Type;
        use crate::parser::parse_impl;
        let src = "impl Entity { fn update(self) { } fn default() -> Self { 0 } }";
        let b = parse_impl(src).unwrap();
        assert_eq!(b.methods.len(), 2);
        // first: method with self receiver
        assert_eq!(b.methods[0].params.len(), 1);
        assert!(matches!(b.methods[0].params[0].ty, Type::SelfTy(_)));
        // second: associated fn, no receiver
        assert!(b.methods[1].params.is_empty());
    }

    #[test]
    fn parse_impl_non_fn_rejected() {
        use crate::parser::parse_impl;
        let err = parse_impl("impl Foo { let x = 5; }").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("§22"), "must cite §22: {msg}");
    }

    #[test]
    fn parse_impl_integrated_with_top_level_fn() {
        use crate::ast::Item;
        let src = "fn free() -> i32 { 1 } impl Foo { fn bar(self) { } }";
        let tokens = Lexer::new(src).lex().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        assert_eq!(ast.items.len(), 2);
        assert!(matches!(ast.items[0], Item::Function(_)));
        assert!(matches!(ast.items[1], Item::Impl(_)));
    }

    // --- Struct declarations ---

    #[test]
    fn parse_struct_bare() {
        use crate::parser::parse_struct;
        let def = parse_struct("struct Point { x: f64, y: f64 }").unwrap();
        assert_eq!(def.name, "Point");
        assert!(def.copy_move.is_none());
        assert!(def.generic_params.is_empty());
        assert_eq!(def.fields.len(), 2);
        assert_eq!(def.fields[0].name, "x");
        assert_eq!(def.fields[1].name, "y");
    }

    #[test]
    fn parse_struct_copy_modifier() {
        use crate::ast::CopyMove;
        use crate::parser::parse_struct;
        let def = parse_struct("copy struct Handle { fd: i32 }").unwrap();
        assert_eq!(def.name, "Handle");
        assert_eq!(def.copy_move, Some(CopyMove::Copy));
        assert_eq!(def.fields.len(), 1);
        assert_eq!(def.fields[0].name, "fd");
    }

    #[test]
    fn parse_struct_move_modifier() {
        use crate::ast::CopyMove;
        use crate::parser::parse_struct;
        let def = parse_struct("move struct Fd { raw: i32 }").unwrap();
        assert_eq!(def.name, "Fd");
        assert_eq!(def.copy_move, Some(CopyMove::Move));
    }

    #[test]
    fn parse_struct_generic() {
        use crate::parser::parse_struct;
        let def = parse_struct("struct Cache<T> { val: T }").unwrap();
        assert_eq!(def.name, "Cache");
        assert_eq!(def.generic_params, vec!["T"]);
        assert_eq!(def.fields.len(), 1);
        assert_eq!(def.fields[0].name, "val");
    }

    #[test]
    fn parse_struct_trailing_comma() {
        use crate::parser::parse_struct;
        let def = parse_struct("struct S { a: i32, b: bool, }").unwrap();
        assert_eq!(def.fields.len(), 2);
    }

    #[test]
    fn parse_struct_empty_body() {
        use crate::parser::parse_struct;
        let def = parse_struct("struct Empty { }").unwrap();
        assert_eq!(def.name, "Empty");
        assert!(def.fields.is_empty());
    }

    #[test]
    fn parse_struct_integrated_with_fn() {
        use crate::ast::Item;
        let src = "struct Pt { x: f64 } fn f() { }";
        let tokens = Lexer::new(src).lex().unwrap();
        let ast = Parser::new(tokens).parse().unwrap();
        assert_eq!(ast.items.len(), 2);
        assert!(matches!(ast.items[0], Item::Struct(_)));
        assert!(matches!(ast.items[1], Item::Function(_)));
    }
}
