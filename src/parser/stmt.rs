use crate::ast::{BinOp, Block, Expr, Stmt};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(crate) fn parse_block(&mut self) -> Result<Block<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::LBrace)?;

        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr<'src>>> = None;

        loop {
            if matches!(self.peek(), Token::RBrace) {
                break;
            }

            let stmt = self.parse_stmt()?;
            match stmt {
                // Expression immediately before `}` without `;` is the tail value.
                Stmt::Expr { expr, has_semicolon: false, .. } => {
                    tail = Some(expr);
                    break;
                }
                s => stmts.push(s),
            }
        }

        let end = self.eat(&Token::RBrace)?.end;
        Ok(Block { stmts, tail, span: Span { start, end } })
    }

    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt<'src>, ParseError> {
        match self.peek() {
            Token::Let      => self.parse_let(),
            Token::Const    => self.parse_const(),
            Token::Return   => self.parse_return(),
            Token::Break    => self.parse_break(),
            Token::Continue => self.parse_continue(),
            _               => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Let)?;
        let mutable = if matches!(self.peek(), Token::Mut) { self.advance(); true } else { false };
        let (name, name_span) = self.eat_ident()?;
        let ty = if matches!(self.peek(), Token::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.eat(&Token::Equals)?;
        let init = Box::new(self.parse_expr()?);
        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Let { mutable, name, name_span, ty, init, span: Span { start, end } })
    }

    fn parse_const(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Const)?;
        let (name, name_span) = self.eat_ident()?;
        self.eat(&Token::Colon)?;
        let ty = self.parse_type()?;
        self.eat(&Token::Equals)?;
        let init = Box::new(self.parse_expr()?);
        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Const { name, name_span, ty, init, span: Span { start, end } })
    }

    fn parse_return(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Return)?;
        let value = if matches!(self.peek(), Token::Semicolon) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Return { value, span: Span { start, end } })
    }

    fn parse_break(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Break)?;
        let value = if matches!(self.peek(), Token::Semicolon) {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };
        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Break { value, span: Span { start, end } })
    }

    fn parse_continue(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Continue)?;
        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Continue { span: Span { start, end } })
    }

    /// Expression statement — also handles assignment.
    fn parse_expr_stmt(&mut self) -> Result<Stmt<'src>, ParseError> {
        let start = self.peek_span().start;
        let expr = self.parse_expr()?;

        let assign_op: Option<Option<BinOp>> = match self.peek() {
            Token::Equals    => Some(None),
            Token::PlusEq    => Some(Some(BinOp::Add)),
            Token::MinusEq   => Some(Some(BinOp::Sub)),
            Token::StarEq    => Some(Some(BinOp::Mul)),
            Token::SlashEq   => Some(Some(BinOp::Div)),
            Token::PercentEq => Some(Some(BinOp::Mod)),
            _ => None,
        };

        if let Some(op) = assign_op {
            self.advance();
            let value = Box::new(self.parse_expr()?);
            let end = self.eat(&Token::Semicolon)?.end;
            return Ok(Stmt::Assign { target: Box::new(expr), op, value, span: Span { start, end } });
        }

        // Block-like expressions (if/while/loop/for/match/block) don't require a
        // trailing `;` — consistent with Rust and most expression-oriented languages.
        let is_block_like = matches!(
            expr,
            Expr::If { .. }
                | Expr::While { .. }
                | Expr::Loop { .. }
                | Expr::For { .. }
                | Expr::Match { .. }
                | Expr::Block(..)
        );
        if is_block_like {
            // Explicit `;` means "discard value" — the expression is a statement, never the tail.
            // No `;` promotes to tail only when the next token is `}` (the enclosing block close).
            let explicit_semi = if matches!(self.peek(), Token::Semicolon) {
                self.advance();
                true
            } else {
                false
            };
            let has_semicolon = explicit_semi || !matches!(self.peek(), Token::RBrace | Token::Eof);
            let end = expr.span().end;
            return Ok(Stmt::Expr { expr: Box::new(expr), has_semicolon, span: Span { start, end } });
        }

        // Tail position: expression immediately before `}` — no `;` needed.
        if matches!(self.peek(), Token::RBrace | Token::Eof) {
            let end = expr.span().end;
            return Ok(Stmt::Expr { expr: Box::new(expr), has_semicolon: false, span: Span { start, end } });
        }

        let end = self.eat(&Token::Semicolon)?.end;
        Ok(Stmt::Expr { expr: Box::new(expr), has_semicolon: true, span: Span { start, end } })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ast::{BinOp, Stmt};
    use crate::parser::parse_stmt;

    #[test]
    fn parse_let_stmt() {
        let stmt = parse_stmt("let x: i32 = 5;").unwrap();
        assert!(matches!(stmt, Stmt::Let { mutable: false, name: "x", ty: Some(_), .. }));
    }

    #[test]
    fn parse_let_mut_stmt() {
        let stmt = parse_stmt("let mut count: i32 = 0;").unwrap();
        assert!(matches!(stmt, Stmt::Let { mutable: true, name: "count", .. }));
    }

    #[test]
    fn parse_let_inferred_type() {
        let stmt = parse_stmt("let x = 5;").unwrap();
        assert!(matches!(stmt, Stmt::Let { ty: None, .. }));
    }

    #[test]
    fn parse_const_stmt() {
        let stmt = parse_stmt("const MAX: i32 = 100;").unwrap();
        assert!(matches!(stmt, Stmt::Const { name: "MAX", .. }));
    }

    #[test]
    fn parse_return_stmt() {
        let stmt = parse_stmt("return 42;").unwrap();
        assert!(matches!(stmt, Stmt::Return { value: Some(_), .. }));
    }

    #[test]
    fn parse_return_void() {
        let stmt = parse_stmt("return;").unwrap();
        assert!(matches!(stmt, Stmt::Return { value: None, .. }));
    }

    #[test]
    fn parse_break_value() {
        let stmt = parse_stmt("break 5;").unwrap();
        assert!(matches!(stmt, Stmt::Break { value: Some(_), .. }));
    }

    #[test]
    fn parse_continue_stmt() {
        let stmt = parse_stmt("continue;").unwrap();
        assert!(matches!(stmt, Stmt::Continue { .. }));
    }

    #[test]
    fn parse_expr_stmt() {
        let stmt = parse_stmt("foo();").unwrap();
        assert!(matches!(stmt, Stmt::Expr { has_semicolon: true, .. }));
    }

    #[test]
    fn parse_expr_stmt_no_semicolon_at_eof() {
        // parse_expr_stmt sets has_semicolon: false at Eof (no enclosing block)
        let stmt = parse_stmt("foo()").unwrap();
        assert!(matches!(stmt, Stmt::Expr { has_semicolon: false, .. }));
    }

    #[test]
    fn block_tail_expr_no_semicolon() {
        use crate::parser::parse_block;
        let block = parse_block("{ foo() }").unwrap();
        assert!(block.tail.is_some());
        assert!(block.stmts.is_empty());
    }

    #[test]
    fn block_expr_stmt_with_semicolon() {
        use crate::parser::parse_block;
        let block = parse_block("{ foo(); }").unwrap();
        assert!(block.tail.is_none());
        assert!(matches!(block.stmts[0], Stmt::Expr { has_semicolon: true, .. }));
    }

    #[test]
    fn parse_assignment() {
        let stmt = parse_stmt("x = 5;").unwrap();
        assert!(matches!(stmt, Stmt::Assign { op: None, .. }));
    }

    #[test]
    fn parse_compound_assignment() {
        let stmt = parse_stmt("x += 1;").unwrap();
        assert!(matches!(stmt, Stmt::Assign { op: Some(BinOp::Add), .. }));
    }
}

