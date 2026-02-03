//! Perfect hash keyword lookup

use phf::phf_map;
use crate::lexer::TokenType;

pub static KEYWORDS: phf::Map<&'static str, TokenType> = phf_map! {
    // Functions & Variables
    "fn" => TokenType::Fn,
    "let" => TokenType::Let,
    "mut" => TokenType::Mut,
    "const" => TokenType::Const,

    // Control Flow
    "if" => TokenType::If,
    "elif" => TokenType::Elif,
    "else" => TokenType::Else,
    "match" => TokenType::Match,
    "where" => TokenType::Where,
    "for" => TokenType::For,
    "in" => TokenType::In,
    "while" => TokenType::While,
    "loop" => TokenType::Loop,
    "break" => TokenType::Break,
    "continue" => TokenType::Continue,
    "return" => TokenType::Return,

    // Imports
    "summon" => TokenType::Summon,
    "from" => TokenType::From,
    "as" => TokenType::As,
    "package" => TokenType::Package,

    // Async
    "async" => TokenType::Async,
    "await" => TokenType::Await,

    // Error Handling
    "try" => TokenType::Try,
    "catch" => TokenType::Catch,
    "fail" => TokenType::Fail,

    // Types
    "struct" => TokenType::Struct,
    "enum" => TokenType::Enum,
    "trait" => TokenType::Trait,
    "impl" => TokenType::Impl,

    // Modifiers
    "pub" => TokenType::Pub,
    "edge" => TokenType::Edge,
    "unsafe" => TokenType::Unsafe,
    "with" => TokenType::With,
    "defer" => TokenType::Defer,

    // Logical
    "and" => TokenType::And,
    "or" => TokenType::Or,
    "not" => TokenType::Not,

    // Literals
    "true" => TokenType::True,
    "false" => TokenType::False,
    "null" => TokenType::Null,
    "self" => TokenType::SelfKw,

    // Properties
    "get" => TokenType::Get,
    "set" => TokenType::Set,
};

#[inline]
pub fn get_keyword(word: &str) -> Option<TokenType> {
    KEYWORDS.get(word).cloned()
}