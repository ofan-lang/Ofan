use crate::ast::{BinOp, Expr, Literal, UnaryOp};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        self.parse_expr_prec(0)
    }

    /// Binding powers for binary operators (left BP, right BP).
    /// Higher = tighter binding. Left-associative: right = left + 1.
    fn binary_bp(tok: &Token<'_>) -> Option<(u8, u8)> {
        Some(match tok {
            Token::QuestionColon              => (1, 2),
            Token::PipePipe                   => (3, 4),
            Token::AmpAmp                     => (5, 6),
            Token::EqEq | Token::BangEq       => (7, 8),
            Token::Lt | Token::Gt
            | Token::LtEq | Token::GtEq       => (9, 10),
            Token::Pipe                       => (11, 12),
            Token::Caret                      => (13, 14),
            Token::Amp                        => (15, 16),
            Token::Shl | Token::Shr           => (17, 18),
            Token::Plus | Token::Minus        => (19, 20),
            Token::Star | Token::Slash
            | Token::Percent                  => (21, 22),
            _ => return None,
        })
    }

    fn tok_to_binop(tok: &Token<'_>) -> BinOp {
        match tok {
            Token::QuestionColon => BinOp::Fallback,
            Token::PipePipe      => BinOp::Or,
            Token::AmpAmp        => BinOp::And,
            Token::EqEq          => BinOp::Eq,
            Token::BangEq        => BinOp::Ne,
            Token::Lt            => BinOp::Lt,
            Token::Gt            => BinOp::Gt,
            Token::LtEq          => BinOp::Le,
            Token::GtEq          => BinOp::Ge,
            Token::Pipe          => BinOp::BitOr,
            Token::Caret         => BinOp::BitXor,
            Token::Amp           => BinOp::BitAnd,
            Token::Shl           => BinOp::Shl,
            Token::Shr           => BinOp::Shr,
            Token::Plus          => BinOp::Add,
            Token::Minus         => BinOp::Sub,
            Token::Star          => BinOp::Mul,
            Token::Slash         => BinOp::Div,
            Token::Percent       => BinOp::Mod,
            _ => unreachable!("tok_to_binop called on non-binary token"),
        }
    }

    fn parse_expr_prec(&mut self, min_bp: u8) -> Result<Expr<'src>, ParseError> {
        let mut lhs = self.parse_unary()?;

        while let Some((left_bp, right_bp)) = Self::binary_bp(self.peek()) {
            if left_bp < min_bp {
                break;
            }
            let op = Self::tok_to_binop(self.peek());
            self.advance();
            let rhs = self.parse_expr_prec(right_bp)?;
            let span = Span { start: lhs.span().start, end: rhs.span().end };
            lhs = Expr::Binary { op, left: Box::new(lhs), right: Box::new(rhs), span };
        }

        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        match self.peek() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                let end = expr.span().end;
                Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(expr), span: Span { start, end } })
            }
            Token::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                let end = expr.span().end;
                Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(expr), span: Span { start, end } })
            }
            Token::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                let end = expr.span().end;
                Ok(Expr::Unary { op: UnaryOp::BitNot, expr: Box::new(expr), span: Span { start, end } })
            }
            Token::Amp => {
                self.advance();
                let mutable = if matches!(self.peek(), Token::Mut) { self.advance(); true } else { false };
                let expr = self.parse_unary()?;
                let end = expr.span().end;
                let op = if mutable { UnaryOp::BorrowMut } else { UnaryOp::Borrow };
                Ok(Expr::Unary { op, expr: Box::new(expr), span: Span { start, end } })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr<'src>, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                Token::Question => {
                    let end = self.advance().1.end;
                    let span = Span { start: expr.span().start, end };
                    expr = Expr::Propagate { expr: Box::new(expr), span };
                }
                Token::Dot => {
                    let _ = self.advance();
                    let (field, field_span) = self.eat_ident()?;
                    let start = expr.span().start;

                    if matches!(self.peek(), Token::LParen) {
                        self.advance();
                        let args = self.parse_call_args()?;
                        let end = self.eat(&Token::RParen)?.end;
                        expr = Expr::MethodCall {
                            object: Box::new(expr),
                            method: field,
                            method_span: field_span,
                            args,
                            span: Span { start, end },
                        };
                    } else {
                        let span = Span { start, end: field_span.end };
                        expr = Expr::Field { object: Box::new(expr), field, field_span, span };
                    }
                }
                Token::LParen => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    let end = self.eat(&Token::RParen)?.end;
                    let span = Span { start: expr.span().start, end };
                    expr = Expr::Call { callee: Box::new(expr), args, span };
                }
                Token::As => {
                    self.advance();
                    let ty = self.parse_type()?;
                    let span = Span { start: expr.span().start, end: ty.span().end };
                    expr = Expr::Cast { expr: Box::new(expr), ty: Box::new(ty), span };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr<'src>>, ParseError> {
        let mut args = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            match self.peek() {
                Token::Comma => { self.advance(); }
                Token::RParen => break,
                _ => return Err(self.error_expected("`,` or `)`", Some("add `,` to separate arguments or `)` to close the argument list"))),
            }
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr<'src>, ParseError> {
        let span = self.peek_span();
        match self.peek() {
            Token::Integer(_) => {
                let (tok, s) = self.advance();
                if let Token::Integer(n) = tok { Ok(Expr::Literal(Literal::Integer(n), s)) }
                else { unreachable!() }
            }
            Token::Float(_) => {
                let (tok, s) = self.advance();
                if let Token::Float(n) = tok { Ok(Expr::Literal(Literal::Float(n), s)) }
                else { unreachable!() }
            }
            Token::True  => { self.advance(); Ok(Expr::Literal(Literal::Bool(true), span)) }
            Token::False => { self.advance(); Ok(Expr::Literal(Literal::Bool(false), span)) }
            Token::Str(_) => {
                let (tok, s) = self.advance();
                if let Token::Str(raw) = tok { Ok(Expr::Literal(Literal::Str(raw), s)) }
                else { unreachable!() }
            }
            Token::Char(_) => {
                let (tok, s) = self.advance();
                if let Token::Char(c) = tok { Ok(Expr::Literal(Literal::Char(c), s)) }
                else { unreachable!() }
            }
            Token::Ident(_) => {
                let (name, s) = self.eat_ident()?;
                Ok(Expr::Ident(name, s))
            }
            Token::SelfKw => {
                let s = self.advance().1;
                Ok(Expr::Ident("self", s))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.eat(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBrace => {
                let block = self.parse_block()?;
                Ok(Expr::Block(Box::new(block)))
            }
            Token::If    => self.parse_if_expr(),
            Token::While => self.parse_while_expr(),
            Token::Loop  => self.parse_loop_expr(),
            Token::For   => self.parse_for_expr(),
            Token::Match => self.parse_match_expr(),
            _ => Err(self.error_expected("an expression", Some("expressions can start with a literal, identifier, `(`, or a keyword like `if`/`match`/`loop`"))),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ast::{BinOp, Expr, Literal, UnaryOp};
    use crate::parser::parse_expr;

    // --- Literals ---

    #[test]
    fn parse_integer_literal() {
        assert!(matches!(parse_expr("42").unwrap(), Expr::Literal(Literal::Integer(42), _)));
    }

    #[test]
    fn parse_float_literal() {
        assert!(matches!(parse_expr("3.14").unwrap(), Expr::Literal(Literal::Float(_), _)));
    }

    #[test]
    fn parse_bool_literals() {
        assert!(matches!(parse_expr("true").unwrap(), Expr::Literal(Literal::Bool(true), _)));
        assert!(matches!(parse_expr("false").unwrap(), Expr::Literal(Literal::Bool(false), _)));
    }

    #[test]
    fn parse_string_literal() {
        assert!(matches!(parse_expr(r#""hello""#).unwrap(), Expr::Literal(Literal::Str("hello"), _)));
    }

    #[test]
    fn parse_char_literal() {
        assert!(matches!(parse_expr("'x'").unwrap(), Expr::Literal(Literal::Char('x'), _)));
    }

    #[test]
    fn parse_ident() {
        assert!(matches!(parse_expr("foo").unwrap(), Expr::Ident("foo", _)));
    }

    // --- Unary ---

    #[test]
    fn parse_unary_neg() {
        assert!(matches!(parse_expr("-5").unwrap(), Expr::Unary { op: UnaryOp::Neg, .. }));
    }

    #[test]
    fn parse_unary_not() {
        assert!(matches!(parse_expr("!flag").unwrap(), Expr::Unary { op: UnaryOp::Not, .. }));
    }

    #[test]
    fn parse_unary_bitnot() {
        assert!(matches!(parse_expr("~mask").unwrap(), Expr::Unary { op: UnaryOp::BitNot, .. }));
    }

    #[test]
    fn parse_borrow() {
        assert!(matches!(parse_expr("&x").unwrap(), Expr::Unary { op: UnaryOp::Borrow, .. }));
    }

    #[test]
    fn parse_borrow_mut() {
        assert!(matches!(parse_expr("&mut x").unwrap(), Expr::Unary { op: UnaryOp::BorrowMut, .. }));
    }

    // --- Binary / precedence ---

    #[test]
    fn parse_binary_add() {
        assert!(matches!(parse_expr("a + b").unwrap(), Expr::Binary { op: BinOp::Add, .. }));
    }

    #[test]
    fn parse_precedence_mul_over_add() {
        let expr = parse_expr("a + b * c").unwrap();
        if let Expr::Binary { op: BinOp::Add, right, .. } = expr {
            assert!(matches!(*right, Expr::Binary { op: BinOp::Mul, .. }));
        } else { panic!("expected Add at top"); }
    }

    #[test]
    fn parse_precedence_logical() {
        let expr = parse_expr("a || b && c").unwrap();
        if let Expr::Binary { op: BinOp::Or, right, .. } = expr {
            assert!(matches!(*right, Expr::Binary { op: BinOp::And, .. }));
        } else { panic!("expected Or at top"); }
    }

    #[test]
    fn parse_precedence_comparison_over_logical() {
        let expr = parse_expr("a && b == c").unwrap();
        if let Expr::Binary { op: BinOp::And, right, .. } = expr {
            assert!(matches!(*right, Expr::Binary { op: BinOp::Eq, .. }));
        } else { panic!("expected And at top"); }
    }

    #[test]
    fn parse_grouped_expr() {
        assert!(matches!(parse_expr("(a + b) * c").unwrap(), Expr::Binary { op: BinOp::Mul, .. }));
    }

    #[test]
    fn parse_fallback_operator() {
        assert!(matches!(parse_expr("opt ?: 0").unwrap(), Expr::Binary { op: BinOp::Fallback, .. }));
    }

    #[test]
    fn parse_fallback_left_assoc() {
        let expr = parse_expr("a ?: b ?: c").unwrap();
        if let Expr::Binary { op: BinOp::Fallback, left, .. } = expr {
            assert!(matches!(*left, Expr::Binary { op: BinOp::Fallback, .. }));
        } else { panic!("expected Fallback at top"); }
    }

    // --- Postfix ---

    #[test]
    fn parse_propagate() {
        assert!(matches!(parse_expr("result?").unwrap(), Expr::Propagate { .. }));
    }

    #[test]
    fn parse_field_access() {
        assert!(matches!(parse_expr("obj.field").unwrap(), Expr::Field { field: "field", .. }));
    }

    #[test]
    fn parse_method_call() {
        assert!(matches!(parse_expr("obj.method(a, b)").unwrap(), Expr::MethodCall { method: "method", .. }));
    }

    #[test]
    fn parse_function_call() {
        assert!(matches!(parse_expr("foo(1, 2)").unwrap(), Expr::Call { .. }));
    }

    #[test]
    fn parse_cast() {
        assert!(matches!(parse_expr("x as f64").unwrap(), Expr::Cast { .. }));
    }

    #[test]
    fn parse_chained_postfix() {
        let expr = parse_expr("f()?.field").unwrap();
        if let Expr::Field { object, .. } = &expr {
            assert!(matches!(**object, Expr::Propagate { .. }));
        } else { panic!("expected Field at top"); }
    }
}
