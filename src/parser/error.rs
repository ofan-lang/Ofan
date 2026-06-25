use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected token at byte {position}: {message}")]
    UnexpectedToken { position: usize, message: String },
    #[error("unexpected end of input")]
    UnexpectedEof,
}
