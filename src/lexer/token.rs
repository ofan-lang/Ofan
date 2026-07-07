use std::fmt;

/// Byte-offset range within the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'src> {
    // Literals
    Integer(i64),
    Float(f64),
    Str(&'src str),
    Char(char),
    Ident(&'src str),

    // Doc comments (preserved for future tooling / doc-gen)
    DocComment(&'src str),

    // Keywords — decided syntax
    Fn,
    Let,
    Mut,
    Const,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    True,
    False,
    Struct,
    Enum,
    Pub,
    Use,
    As,
    Using,
    Static,
    Unsafe,
    // §17 Copy/Move semantics (SYNTAX_SPEC.md §17)
    Copy,
    Move,
    // §18 method receivers and impl blocks (SYNTAX_SPEC.md §18)
    SelfKw,
    Impl,

    // §16 loop syntax (SYNTAX_SPEC.md §16)
    Loop,
    // §21 match / pattern matching (SYNTAX_SPEC.md §21)
    Match,

    // Keywords — reserved ahead of syntax decisions (SYNTAX_SPEC.md §22)
    // Grammar for these constructs is not yet decided; words are reserved now
    // so they cannot be used as identifiers before that decision is made.
    Trait,
    Mod,

    // Operators — arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,

    // Operators — comparison / assignment
    Equals,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,

    // Operators — logical
    AmpAmp,
    PipePipe,
    Bang,

    // Operators — bitwise
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,

    // Operators — misc
    Arrow,
    Question,
    QuestionColon,
    // §21 match arm separator `=>`
    FatArrow,

    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Colon,
    Comma,
    Dot,

    // Sentinel
    Eof,
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Integer(n)    => write!(f, "{n}"),
            Token::Float(n)      => write!(f, "{n}"),
            Token::Str(s)        => write!(f, "\"{s}\""),
            Token::Char(c)       => write!(f, "'{c}'"),
            Token::Ident(s)      => write!(f, "`{s}`"),
            Token::DocComment(_) => write!(f, "`### ...`"),
            Token::Fn        => write!(f, "`fn`"),
            Token::Let       => write!(f, "`let`"),
            Token::Mut       => write!(f, "`mut`"),
            Token::Const     => write!(f, "`const`"),
            Token::If        => write!(f, "`if`"),
            Token::Else      => write!(f, "`else`"),
            Token::While     => write!(f, "`while`"),
            Token::For       => write!(f, "`for`"),
            Token::In        => write!(f, "`in`"),
            Token::Return    => write!(f, "`return`"),
            Token::Break     => write!(f, "`break`"),
            Token::Continue  => write!(f, "`continue`"),
            Token::True      => write!(f, "`true`"),
            Token::False     => write!(f, "`false`"),
            Token::Struct    => write!(f, "`struct`"),
            Token::Enum      => write!(f, "`enum`"),
            Token::Pub       => write!(f, "`pub`"),
            Token::Use       => write!(f, "`use`"),
            Token::As        => write!(f, "`as`"),
            Token::Using     => write!(f, "`using`"),
            Token::Static    => write!(f, "`static`"),
            Token::Unsafe    => write!(f, "`unsafe`"),
            Token::Copy      => write!(f, "`copy`"),
            Token::Move      => write!(f, "`move`"),
            Token::SelfKw    => write!(f, "`self`"),
            Token::Impl      => write!(f, "`impl`"),
            Token::Loop      => write!(f, "`loop`"),
            Token::Match     => write!(f, "`match`"),
            Token::Trait     => write!(f, "`trait`"),
            Token::Mod       => write!(f, "`mod`"),
            Token::Plus      => write!(f, "`+`"),
            Token::Minus     => write!(f, "`-`"),
            Token::Star      => write!(f, "`*`"),
            Token::Slash     => write!(f, "`/`"),
            Token::Percent   => write!(f, "`%`"),
            Token::PlusEq    => write!(f, "`+=`"),
            Token::MinusEq   => write!(f, "`-=`"),
            Token::StarEq    => write!(f, "`*=`"),
            Token::SlashEq   => write!(f, "`/=`"),
            Token::PercentEq => write!(f, "`%=`"),
            Token::Equals    => write!(f, "`=`"),
            Token::EqEq      => write!(f, "`==`"),
            Token::BangEq    => write!(f, "`!=`"),
            Token::Lt        => write!(f, "`<`"),
            Token::Gt        => write!(f, "`>`"),
            Token::LtEq      => write!(f, "`<=`"),
            Token::GtEq      => write!(f, "`>=`"),
            Token::AmpAmp    => write!(f, "`&&`"),
            Token::PipePipe  => write!(f, "`||`"),
            Token::Bang      => write!(f, "`!`"),
            Token::Amp       => write!(f, "`&`"),
            Token::Pipe      => write!(f, "`|`"),
            Token::Caret     => write!(f, "`^`"),
            Token::Tilde     => write!(f, "`~`"),
            Token::Shl       => write!(f, "`<<`"),
            Token::Shr       => write!(f, "`>>`"),
            Token::Arrow     => write!(f, "`->`"),
            Token::Question  => write!(f, "`?`"),
            Token::QuestionColon => write!(f, "`?:`"),
            Token::FatArrow  => write!(f, "`=>`"),
            Token::LParen    => write!(f, "`(`"),
            Token::RParen    => write!(f, "`)`"),
            Token::LBrace    => write!(f, "`{{`"),
            Token::RBrace    => write!(f, "`}}`"),
            Token::LBracket  => write!(f, "`[`"),
            Token::RBracket  => write!(f, "`]`"),
            Token::Semicolon => write!(f, "`;`"),
            Token::Colon     => write!(f, "`:`"),
            Token::Comma     => write!(f, "`,`"),
            Token::Dot       => write!(f, "`.`"),
            Token::Eof       => write!(f, "end of file"),
        }
    }
}
