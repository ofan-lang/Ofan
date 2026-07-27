use crate::ast::{Expr, Literal, MatchArm, Pattern};
use crate::lexer::token::{Span, Token};
use crate::parser::{ParseError, Parser};

impl<'src> Parser<'src> {
    pub(crate) fn parse_match_expr(&mut self) -> Result<Expr<'src>, ParseError> {
        let start = self.peek_span().start;
        self.eat(&Token::Match)?;
        let prev = self.no_struct_lit;
        self.no_struct_lit = true;
        let subject = Box::new(self.parse_expr()?);
        self.no_struct_lit = prev;
        self.eat(&Token::LBrace)?;

        let mut arms = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            arms.push(self.parse_match_arm()?);
        }

        let end = self.eat(&Token::RBrace)?.end;
        Ok(Expr::Match { subject, arms, span: Span { start, end } })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm<'src>, ParseError> {
        let start = self.peek_span().start;

        // Optional leading `|` (write-time convenience; formatter removes it per §21)
        if matches!(self.peek(), Token::Pipe) {
            self.advance();
        }

        let pattern = self.parse_pattern_or()?;

        // Reject qualified patterns (EnumName.Variant) — Phase 1 supports bare names only.
        // Without this, `Shape.Circle(r)` parses `Shape` as Pattern::Name and then
        // hits "expected =>, found ." with a misleading "add `=>`" suggestion.
        if matches!(self.peek(), Token::Dot) {
            return Err(self.error_expected(
                "=> or if",
                Some("qualified patterns (EnumName.Variant) are not supported in match arms \
                      — use the bare variant name (Variant) directly; the subject type \
                      determines which enum is searched"),
            ));
        }

        let guard = if matches!(self.peek(), Token::If) {
            self.advance();
            let prev = self.no_struct_lit;
            self.no_struct_lit = true;
            let g = self.parse_expr()?;
            self.no_struct_lit = prev;
            Some(Box::new(g))
        } else {
            None
        };

        self.eat(&Token::FatArrow)?;
        let body = self.parse_expr()?;
        let end = body.span().end;

        // §21: comma required after every arm body; trailing comma on last arm is
        // permitted (not required). Skip the comma when `}` immediately follows.
        if !matches!(self.peek(), Token::RBrace | Token::Eof) {
            self.eat(&Token::Comma)?;
        }

        Ok(MatchArm { pattern, guard, body, span: Span { start, end } })
    }

    /// Parse an or-pattern: `A | B | C`
    fn parse_pattern_or(&mut self) -> Result<Pattern<'src>, ParseError> {
        let start = self.peek_span().start;
        let first = self.parse_pattern_atom()?;

        if !matches!(self.peek(), Token::Pipe) {
            return Ok(first);
        }

        let mut alts = vec![first];
        while matches!(self.peek(), Token::Pipe) {
            self.advance();
            alts.push(self.parse_pattern_atom()?);
        }

        let end = alts.last().unwrap().span().end;
        Ok(Pattern::Or(alts, Span { start, end }))
    }

    fn parse_pattern_atom(&mut self) -> Result<Pattern<'src>, ParseError> {
        let start = self.peek_span().start;
        match self.peek() {
            Token::Ident(s) if *s == "_" => {
                let span = self.advance().1;
                Ok(Pattern::Wildcard(span))
            }
            Token::Integer(_) => {
                let (tok, s) = self.advance();
                if let Token::Integer(n) = tok { Ok(Pattern::Literal(Literal::Integer(n), s)) }
                else { unreachable!() }
            }
            Token::Minus => {
                let _ = self.advance();
                match self.peek() {
                    Token::Integer(_) => {
                        let (tok, end_span) = self.advance();
                        if let Token::Integer(n) = tok {
                            Ok(Pattern::Literal(Literal::Integer(-n), Span { start, end: end_span.end }))
                        } else { unreachable!() }
                    }
                    Token::Float(_) => {
                        let (tok, end_span) = self.advance();
                        if let Token::Float(n) = tok {
                            Ok(Pattern::Literal(Literal::Float(-n), Span { start, end: end_span.end }))
                        } else { unreachable!() }
                    }
                    _ => Err(self.error_expected("a numeric literal after `-`", Some("write a number after `-`, e.g. `-1` or `-2.5`"))),
                }
            }
            Token::Float(_) => {
                let (tok, s) = self.advance();
                if let Token::Float(n) = tok { Ok(Pattern::Literal(Literal::Float(n), s)) }
                else { unreachable!() }
            }
            Token::True  => { let s = self.advance().1; Ok(Pattern::Literal(Literal::Bool(true), s)) }
            Token::False => { let s = self.advance().1; Ok(Pattern::Literal(Literal::Bool(false), s)) }
            Token::Str(_) => {
                let (tok, s) = self.advance();
                if let Token::Str(raw) = tok { Ok(Pattern::Literal(Literal::Str(raw), s)) }
                else { unreachable!() }
            }
            Token::Char(_) => {
                let (tok, s) = self.advance();
                if let Token::Char(c) = tok { Ok(Pattern::Literal(Literal::Char(c), s)) }
                else { unreachable!() }
            }
            Token::Ident(_) => {
                let (name, name_span) = self.eat_ident()?;
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let mut sub_patterns = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        loop {
                            sub_patterns.push(self.parse_pattern_or()?);
                            match self.peek() {
                                Token::Comma => { self.advance(); }
                                Token::RParen => break,
                                _ => return Err(self.error_expected("`,` or `)`", Some("add `,` to separate pattern fields or `)` to close the constructor pattern"))),
                            }
                        }
                    }
                    let end = self.eat(&Token::RParen)?.end;
                    Ok(Pattern::Constructor { name, name_span, sub_patterns, span: Span { start, end } })
                } else {
                    Ok(Pattern::Name(name, name_span))
                }
            }
            Token::LParen => {
                self.advance();
                let pat = self.parse_pattern_or()?;
                self.eat(&Token::RParen)?;
                Ok(pat)
            }
            _ => Err(self.error_expected("a pattern", Some("patterns can be `_`, a literal, a name, or `Name(...)` for constructor patterns"))),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::ast::{Expr, Pattern};
    use crate::parser::parse_expr;

    #[test]
    fn parse_match_basic() {
        let expr = parse_expr("match opt { Some(x) => x, None => 0, }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_wildcard() {
        let expr = parse_expr("match n { 0 => zero(), _ => other(), }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(arms[1].pattern, Pattern::Wildcard(_)));
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_guard() {
        let src = "match opt { Some(n) if n > 0 => pos(n), Some(n) => neg(n), None => zero(), }";
        let expr = parse_expr(src).unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(arms[0].guard.is_some());
            assert!(arms[1].guard.is_none());
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_or_pattern() {
        let expr = parse_expr("match d { North | South => v(), East | West => h(), }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(arms[0].pattern, Pattern::Or(_, _)));
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_nested_pattern() {
        let expr = parse_expr("match opt { Some(Some(x)) => x, Some(None) => 0, None => 1, }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 3);
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_block_arm_body() {
        let expr = parse_expr("match x { 1 => { let y = 2; y }, _ => 0, }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert!(matches!(arms[0].body, Expr::Block(_)));
        } else { panic!("expected Match"); }
    }

    #[test]
    fn parse_match_no_trailing_comma() {
        // §21: trailing comma on last arm is permitted, not required
        let expr = parse_expr("match x { 1 => 1, _ => 0 }").unwrap();
        if let Expr::Match { arms, .. } = expr {
            assert_eq!(arms.len(), 2);
        } else { panic!("expected Match"); }
    }
}
