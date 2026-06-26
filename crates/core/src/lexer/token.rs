// src/lexer/token.rs

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // ── Core keywords ─────────────────────────────────────────────
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

    // ── Contextual / declaration keywords ────────────────────────
    Extend,
    TypeKw,
    Extract,
    Using,
    Lifetime,
    Tier,
    High, Mid, Low,
    Arena,
    Pool,
    Gc,
    Heap,

    // ── Built-in collection type keywords ────────────────────────
    KwList,
    KwDictionary,
    KwSet,
    KwQueue,
    KwStack,

    // ── Wildcard / infer placeholder ─────────────────────────────
    Underscore,

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

    // ── Arithmetic ───────────────────────────────────────────────
    Plus, Minus, Star, Slash, Percent,

    // ── Bitwise ──────────────────────────────────────────────────
    Amp, Pipe, Caret, Tilde,
    LeftShift, RightShift,

    // ── Comparison ───────────────────────────────────────────────
    Equal, EqualEqual, BangEqual,
    Less, Greater, LessEqual, GreaterEqual,

    // ── Logical ──────────────────────────────────────────────────
    Bang, AmpAmp, PipePipe,

    // ── Compound assignment ───────────────────────────────────────
    PlusEqual, MinusEqual, StarEqual, SlashEqual, PercentEqual,
    AmpEqual, PipeEqual, CaretEqual,
    LeftShiftEqual, RightShiftEqual,

    // ── Range / spread ────────────────────────────────────────────
    DotDot,
    DotDotEqual,
    DotDotDot,

    // ── Special operators ─────────────────────────────────────────
    Question,
    QuestionDot,
    FatArrow,
    ColonEqual,
    PipeArrow,

    // ── Delimiters ────────────────────────────────────────────────
    LeftParen, RightParen,
    LeftBrace, RightBrace,
    LeftBracket, RightBracket,

    // ── Punctuation ──────────────────────────────────────────────
    Comma, Dot, Colon, Semicolon, At,

    // ── Comments ─────────────────────────────────────────────────
    DocComment(String),
    Comment(String),

    // ── Special ──────────────────────────────────────────────────
    Newline,
    Eof,
    Error(String),
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenType::Fn        => write!(f, "fn"),
            TokenType::Let       => write!(f, "let"),
            TokenType::Mut       => write!(f, "mut"),
            TokenType::Const     => write!(f, "const"),
            TokenType::If        => write!(f, "if"),
            TokenType::Elif      => write!(f, "elif"),
            TokenType::Else      => write!(f, "else"),
            TokenType::Match     => write!(f, "match"),
            TokenType::Where     => write!(f, "where"),
            TokenType::For       => write!(f, "for"),
            TokenType::In        => write!(f, "in"),
            TokenType::While     => write!(f, "while"),
            TokenType::Loop      => write!(f, "loop"),
            TokenType::Break     => write!(f, "break"),
            TokenType::Continue  => write!(f, "continue"),
            TokenType::Return    => write!(f, "return"),
            TokenType::Summon    => write!(f, "summon"),
            TokenType::From      => write!(f, "from"),
            TokenType::As        => write!(f, "as"),
            TokenType::Package   => write!(f, "package"),
            TokenType::Async     => write!(f, "async"),
            TokenType::Await     => write!(f, "await"),
            TokenType::Task      => write!(f, "Task"),
            TokenType::Try       => write!(f, "try"),
            TokenType::Catch     => write!(f, "catch"),
            TokenType::Fail      => write!(f, "fail"),
            TokenType::Struct    => write!(f, "struct"),
            TokenType::Enum      => write!(f, "enum"),
            TokenType::Trait     => write!(f, "trait"),
            TokenType::Impl      => write!(f, "impl"),
            TokenType::Pub       => write!(f, "pub"),
            TokenType::Edge      => write!(f, "edge"),
            TokenType::Unsafe    => write!(f, "unsafe"),
            TokenType::With      => write!(f, "with"),
            TokenType::Defer     => write!(f, "defer"),
            TokenType::And       => write!(f, "and"),
            TokenType::Or        => write!(f, "or"),
            TokenType::Not       => write!(f, "not"),
            TokenType::True      => write!(f, "true"),
            TokenType::False     => write!(f, "false"),
            TokenType::Null      => write!(f, "null"),
            TokenType::SelfKw    => write!(f, "self"),
            TokenType::Get       => write!(f, "get"),
            TokenType::Set       => write!(f, "set"),
            TokenType::Extend    => write!(f, "extend"),
            TokenType::TypeKw    => write!(f, "type"),
            TokenType::Extract   => write!(f, "extract"),
            TokenType::Using     => write!(f, "using"),
            TokenType::Lifetime  => write!(f, "lifetime"),
            TokenType::Tier      => write!(f, "tier"),
            TokenType::High      => write!(f, "high"),
            TokenType::Mid       => write!(f, "mid"),
            TokenType::Low       => write!(f, "low"),
            TokenType::Arena     => write!(f, "arena"),
            TokenType::Pool      => write!(f, "pool"),
            TokenType::Gc        => write!(f, "gc"),
            TokenType::Heap      => write!(f, "heap"),
            TokenType::KwList        => write!(f, "List"),
            TokenType::KwDictionary  => write!(f, "Dictionary"),
            TokenType::KwSet         => write!(f, "Set"),
            TokenType::KwQueue       => write!(f, "Queue"),
            TokenType::KwStack       => write!(f, "Stack"),
            TokenType::Underscore    => write!(f, "_"),
            TokenType::IntLit(n)     => write!(f, "{}", n),
            TokenType::FloatLit(v)   => write!(f, "{}f", v),
            TokenType::DoubleLit(v)  => write!(f, "{}", v),
            TokenType::StringLit(s)  => write!(f, "\"{}\"", s),
            TokenType::InterpolatedString(_) => write!(f, "$\"...\""),
            TokenType::VerbatimString(s)     => write!(f, "@\"{}\"", s),
            TokenType::CharLit(c)    => write!(f, "'{}'", c),
            TokenType::Ident(s)      => write!(f, "{}", s),
            TokenType::Plus          => write!(f, "+"),
            TokenType::Minus         => write!(f, "-"),
            TokenType::Star          => write!(f, "*"),
            TokenType::Slash         => write!(f, "/"),
            TokenType::Percent       => write!(f, "%"),
            TokenType::Amp           => write!(f, "&"),
            TokenType::Pipe          => write!(f, "|"),
            TokenType::Caret         => write!(f, "^"),
            TokenType::Tilde         => write!(f, "~"),
            TokenType::LeftShift     => write!(f, "<<"),
            TokenType::RightShift    => write!(f, ">>"),
            TokenType::Equal         => write!(f, "="),
            TokenType::EqualEqual    => write!(f, "=="),
            TokenType::BangEqual     => write!(f, "!="),
            TokenType::Less          => write!(f, "<"),
            TokenType::Greater       => write!(f, ">"),
            TokenType::LessEqual     => write!(f, "<="),
            TokenType::GreaterEqual  => write!(f, ">="),
            TokenType::Bang          => write!(f, "!"),
            TokenType::AmpAmp        => write!(f, "&&"),
            TokenType::PipePipe      => write!(f, "||"),
            TokenType::PlusEqual     => write!(f, "+="),
            TokenType::MinusEqual    => write!(f, "-="),
            TokenType::StarEqual     => write!(f, "*="),
            TokenType::SlashEqual    => write!(f, "/="),
            TokenType::PercentEqual  => write!(f, "%="),
            TokenType::AmpEqual      => write!(f, "&="),
            TokenType::PipeEqual     => write!(f, "|="),
            TokenType::CaretEqual    => write!(f, "^="),
            TokenType::LeftShiftEqual  => write!(f, "<<="),
            TokenType::RightShiftEqual => write!(f, ">>="),
            TokenType::DotDot        => write!(f, ".."),
            TokenType::DotDotEqual   => write!(f, "..="),
            TokenType::DotDotDot     => write!(f, "..."),
            TokenType::Question      => write!(f, "?"),
            TokenType::QuestionDot   => write!(f, "?."),
            TokenType::FatArrow      => write!(f, "=>"),
            TokenType::ColonEqual    => write!(f, ":="),
            TokenType::PipeArrow     => write!(f, "|>"),
            TokenType::LeftParen     => write!(f, "("),
            TokenType::RightParen    => write!(f, ")"),
            TokenType::LeftBrace     => write!(f, "{{"),
            TokenType::RightBrace    => write!(f, "}}"),
            TokenType::LeftBracket   => write!(f, "["),
            TokenType::RightBracket  => write!(f, "]"),
            TokenType::Comma         => write!(f, ","),
            TokenType::Dot           => write!(f, "."),
            TokenType::Colon         => write!(f, ":"),
            TokenType::Semicolon     => write!(f, ";"),
            TokenType::At            => write!(f, "@"),
            TokenType::DocComment(s) => write!(f, "/** {} */", s),
            TokenType::Comment(s)    => write!(f, "/* {} */", s),
            TokenType::Newline       => write!(f, "<newline>"),
            TokenType::Eof           => write!(f, "<EOF>"),
            TokenType::Error(s)      => write!(f, "<error: {}>", s),
        }
    }
}

/// Parts of an interpolated string `$"..."`.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    Text(String),
    Expr(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Hash)]
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
    pub fn len(&self) -> usize { self.end - self.start }
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start:  self.start.min(other.start),
            end:    self.end.max(other.end),
            line:   self.line,
            column: self.column,
        }
    }
    pub fn at(offset: usize) -> Self {
        Span { start: offset, end: offset, line: 0, column: 0 }
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
        Token { kind: TokenType::Error(message.clone()), span, lexeme: message }
    }
    pub fn is_error(&self) -> bool {
        matches!(self.kind, TokenType::Error(_))
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} @{}:{}", self.kind, self.span.line, self.span.column)
    }
                }
