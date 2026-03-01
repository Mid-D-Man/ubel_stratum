// src/lexer/token.rs
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // ── Keywords ─────────────────────────────────────────────────
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

    // ── Literals ─────────────────────────────────────────────────
    IntLit(i64),
    FloatLit(f32),
    DoubleLit(f64),
    StringLit(String),
    InterpolatedString(Vec<InterpolationPart>),
    VerbatimString(String),
    CharLit(char),

    // ── Identifiers ──────────────────────────────────────────────
    Ident(String),

    // ── Arithmetic operators ─────────────────────────────────────
    Plus, Minus, Star, Slash, Percent,

    // ── Bitwise operators ─────────────────────────────────────────
    Amp, Pipe, Caret, Tilde,
    LeftShift, RightShift,

    // ── Comparison operators ──────────────────────────────────────
    Equal, EqualEqual, BangEqual,
    Less, Greater, LessEqual, GreaterEqual,

    // ── Logical operators ─────────────────────────────────────────
    Bang, AmpAmp, PipePipe,

    // ── Assignment operators ──────────────────────────────────────
    PlusEqual, MinusEqual, StarEqual, SlashEqual, PercentEqual,
    AmpEqual, PipeEqual, CaretEqual,
    LeftShiftEqual, RightShiftEqual,

    // ── Range operators ───────────────────────────────────────────
    DotDot,         // ..   exclusive range
    DotDotEqual,    // ..=  inclusive range
    DotDotDot,      // ...  rest / spread in destructuring

    // ── Special operators ─────────────────────────────────────────
    Question,       // ?
    QuestionDot,    // ?.
    FatArrow,       // =>
    ColonEqual,     // :=
    PipeArrow,      // |>  pipe operator

    // ── Delimiters ────────────────────────────────────────────────
    LeftParen, RightParen,
    LeftBrace, RightBrace,
    LeftBracket, RightBracket,

    // ── Punctuation ───────────────────────────────────────────────
    Comma, Dot, Colon, Semicolon, At,

    // ── Comments ──────────────────────────────────────────────────
    DocComment(String),
    Comment(String),

    // ── Special ───────────────────────────────────────────────────
    Newline,
    Eof,
    Error(String),
}

/// Parts of an interpolated string `$"..."`.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    Text(String),
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
               self.kind, self.lexeme, self.span.line, self.span.column)
    }
        }
