use crate::lexer::Span;
use std::fmt;

#[derive(Debug)]
pub enum ParseError {
    UnexpectedToken {
        span: Span,
        found: String,
        expected: String,
        suggestion: Option<String>,
    },
    UnexpectedEof {
        expected: String,
        suggestion: Option<String>,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken { span, found, expected, suggestion } => {
                write!(f, "unexpected token at byte {}: expected {expected}, found {found}", span.start)?;
                if let Some(s) = suggestion {
                    write!(f, " — {s}")?;
                }
                Ok(())
            }
            ParseError::UnexpectedEof { expected, suggestion } => {
                write!(f, "unexpected end of file: expected {expected}")?;
                if let Some(s) = suggestion {
                    write!(f, " — {s}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ParseError {}
