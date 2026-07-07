use crate::ast::{BorrowKind, Expr};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(crate) fn parse_if_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::If)?;
        let condition = Box::new(self.parse_expr()?);
        let then_block = Box::new(self.parse_block()?);

        let else_branch = if matches!(self.peek(), Token::Else) {
            self.advance();
            if matches!(self.peek(), Token::If) {
                Some(Box::new(self.parse_if_expr()?))
            } else {
                let block = self.parse_block()?;
                Some(Box::new(Expr::Block(Box::new(block))))
            }
        } else {
            None
        };

        let end = else_branch.as_ref()
            .map(|e| e.span().end)
            .unwrap_or(then_block.span.end);
        Ok(Expr::If { condition, then_block, else_branch, span: Span { start, end } })
    }

    pub(crate) fn parse_while_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::While)?;
        let condition = Box::new(self.parse_expr()?);
        let body = Box::new(self.parse_block()?);
        let end = body.span.end;
        Ok(Expr::While { condition, body, span: Span { start, end } })
    }

    pub(crate) fn parse_loop_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Loop)?;
        let body = Box::new(self.parse_block()?);
        let end = body.span.end;
        Ok(Expr::Loop { body, span: Span { start, end } })
    }

    pub(crate) fn parse_for_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::For)?;
        let (binding, binding_span) = self.eat_ident()?;
        self.eat(&Token::In)?;

        let borrow = if matches!(self.peek(), Token::Amp) {
            self.advance();
            if matches!(self.peek(), Token::Mut) { self.advance(); Some(BorrowKind::Mut) }
            else { Some(BorrowKind::Shared) }
        } else {
            None
        };

        let iterable = Box::new(self.parse_expr()?);
        let body = Box::new(self.parse_block()?);
        let end = body.span.end;
        Ok(Expr::For { binding, binding_span, borrow, iterable, body, span: Span { start, end } })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ast::{BorrowKind, Expr};
    use crate::parser::parse_expr;

    #[test]
    fn parse_if_expr() {
        assert!(matches!(parse_expr("if x { 1 }").unwrap(), Expr::If { else_branch: None, .. }));
    }

    #[test]
    fn parse_if_else_expr() {
        assert!(matches!(parse_expr("if x { 1 } else { 2 }").unwrap(), Expr::If { else_branch: Some(_), .. }));
    }

    #[test]
    fn parse_if_else_if_chain() {
        let expr = parse_expr("if a { 1 } else if b { 2 } else { 3 }").unwrap();
        if let Expr::If { else_branch: Some(branch), .. } = expr {
            assert!(matches!(*branch, Expr::If { .. }));
        } else { panic!("expected else branch"); }
    }

    #[test]
    fn parse_while_expr() {
        assert!(matches!(parse_expr("while cond { }").unwrap(), Expr::While { .. }));
    }

    #[test]
    fn parse_loop_expr() {
        assert!(matches!(parse_expr("loop { break 42; }").unwrap(), Expr::Loop { .. }));
    }

    #[test]
    fn parse_for_bare() {
        let expr = parse_expr("for item in items { }").unwrap();
        if let Expr::For { borrow, .. } = expr {
            assert_eq!(borrow, None);
        } else { panic!("expected For"); }
    }

    #[test]
    fn parse_for_borrow_shared() {
        let expr = parse_expr("for item in &items { }").unwrap();
        if let Expr::For { borrow, .. } = expr {
            assert_eq!(borrow, Some(BorrowKind::Shared));
        } else { panic!("expected For"); }
    }

    #[test]
    fn parse_for_borrow_mut() {
        let expr = parse_expr("for item in &mut items { }").unwrap();
        if let Expr::For { borrow, .. } = expr {
            assert_eq!(borrow, Some(BorrowKind::Mut));
        } else { panic!("expected For"); }
    }
}
