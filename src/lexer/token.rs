//! Token types - Separate from lexer logic

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // ========================================
    // Keywords
    // ========================================

    Fn, Let, Mut, Const,
    If, Elif, Else, Match, Where,
    For, In, While, Loop,
    Break, Continue, Return,
    Summon, From, As, Package,
    Async, Await, Task,
    Try, Catch, Fail,
    Struct, Enum, Trait, Impl,
    Pub, Edge, Unsafe, With, Defer,
    And, Or, Not,
    True, False, Null, SelfKw,
    Get, Set,

    // Tier annotations
    TierHigh, TierMid, TierLow,

    // Quantum (future)
    Qubit, Hadamard, Oracle,

    // ========================================
    // Literals
    // ========================================

    IntLit(i64),
    FloatLit(f32),       // 3.14f
    DoubleLit(f64),      // 3.14 (default)
    StringLit(String),
    InterpolatedString(Vec<InterpolationPart>), // $"hello {name}"
    VerbatimString(String),                      // @"C:\path"
    CharLit(char),

    // ========================================
    // Identifiers
    // ========================================

    Ident(String),

    // ========================================
    // Operators
    // ========================================

    // Arithmetic
    Plus, Minus, Star, Slash, Percent,

    // Bitwise
    Amp, Pipe, Caret, Tilde,           // & | ^ ~
    LeftShift, RightShift,             // << >>

    // Comparison
    Equal, EqualEqual, BangEqual,
    Less, Greater, LessEqual, GreaterEqual,

    // Logical
    Bang, AmpAmp, PipePipe,

    // Assignment
    PlusEqual, MinusEqual, StarEqual, SlashEqual, PercentEqual,
    AmpEqual, PipeEqual, CaretEqual,   // &= |= ^=
    LeftShiftEqual, RightShiftEqual,   // <<= >>=

    // Special
    Question,      // ?
    QuestionDot,   // ?.
    FatArrow,      // =>
    ColonEqual,    // :=

    // ========================================
    // Delimiters
    // ========================================

    LeftParen, RightParen,       // ( )
    LeftBrace, RightBrace,       // { }
    LeftBracket, RightBracket,   // [ ]

    // ========================================
    // Punctuation
    // ========================================

    Comma, Dot, Colon, Semicolon, At,

    // ========================================
    // Special
    // ========================================

    /// Doc comment (/*! ... */ or /** ... */)
    DocComment(String),

    /// Regular comment
    Comment(String),

    /// Newline (for tracking only, not in token stream)
    Newline,

    /// End of file
    Eof,

    /// Error token (lexer continues after error)
    Error(String),
}

/// String interpolation parts
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    /// Literal text
    Text(String),
    /// Expression to interpolate: {expr}
    Expr(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Span { start, end, line, column }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line,
            column: self.column,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenType,
    pub span: Span,
    pub lexeme: String,
}

impl Token {
    pub fn new(kind: TokenType, span: Span, lexeme: String) -> Self {
        Token { kind, span, lexeme }
    }

    pub fn error(message: String, span: Span) -> Self {
        Token {
            kind: TokenType::Error(message.clone()),
            span,
            lexeme: message,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self.kind, TokenType::Error(_))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} '{}' @{}:{}",
               self.kind,
               self.lexeme,
               self.span.line,
               self.span.column
        )
    }
}