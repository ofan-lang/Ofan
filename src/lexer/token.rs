/// Byte-offset range within the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
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
