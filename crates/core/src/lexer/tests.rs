//! Unit tests for the lexer (complements integration tests in tests/)

use crate::lexer::{tokenize, InterpolationPart, TokenType};

#[test]
fn test_empty_input() {
    let tokens = tokenize("").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenType::Eof);
}

#[test]
fn test_whitespace_only() {
    let tokens = tokenize("   \t  ").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenType::Eof);
}

#[test]
fn test_newlines_stripped() {
    let tokens = tokenize("\n\n\n").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenType::Eof);
}

#[test]
fn test_single_keyword_fn() {
    let tokens = tokenize("fn").unwrap();
    assert_eq!(tokens[0].kind, TokenType::Fn);
}

#[test]
fn test_ident_not_keyword() {
    let tokens = tokenize("foobar").unwrap();
    assert!(matches!(tokens[0].kind, TokenType::Ident(ref s) if s == "foobar"));
}

#[test]
fn test_integer_zero() {
    let tokens = tokenize("0").unwrap();
    assert_eq!(tokens[0].kind, TokenType::IntLit(0));
}

#[test]
fn test_simple_fn_declaration_structure() {
    let input = "fn add(x: int, y: int) int { return x + y }";
    let tokens = tokenize(input).unwrap();
    assert_eq!(tokens[0].kind, TokenType::Fn);
    assert!(matches!(tokens[1].kind, TokenType::Ident(ref s) if s == "add"));
    assert_eq!(tokens[2].kind, TokenType::LeftParen);
    assert!(matches!(tokens[3].kind, TokenType::Ident(ref s) if s == "x"));
    assert_eq!(tokens[4].kind, TokenType::Colon);
}

#[test]
fn test_span_line_tracking() {
    let input = "fn\nlet\nmut";
    let tokens = tokenize(input).unwrap();
    assert_eq!(tokens[0].span.line, 1, "fn should be on line 1");
    assert_eq!(tokens[1].span.line, 2, "let should be on line 2");
    assert_eq!(tokens[2].span.line, 3, "mut should be on line 3");
}

#[test]
fn test_all_tier_keywords() {
    // Tier annotations come through as @tier(high) etc — @ + ident + ( + ident + )
    let input = "@tier(high) @tier(mid) @tier(low)";
    let tokens = tokenize(input).unwrap();
    assert_eq!(tokens[0].kind, TokenType::At);
    assert!(matches!(tokens[1].kind, TokenType::Ident(ref s) if s == "tier"));
}

#[test]
fn test_string_interpolation_parts_count() {
    let input = r#"$"Hello {name} you are {age} years old""#;
    let tokens = tokenize(input).unwrap();
    match &tokens[0].kind {
        TokenType::InterpolatedString(parts) => {
            // "Hello ", {name}, " you are ", {age}, " years old"
            assert_eq!(parts.len(), 5);
        }
        _ => panic!("Expected InterpolatedString, got {:?}", tokens[0].kind),
    }
}

#[test]
fn test_underscore_ident() {
    let tokens = tokenize("_unused").unwrap();
    assert!(matches!(tokens[0].kind, TokenType::Ident(ref s) if s == "_unused"));
}

#[test]
fn test_compound_operators_no_overlap() {
    // Ensure <<= is not parsed as << followed by =
    let tokens = tokenize("<<=").unwrap();
    assert_eq!(tokens[0].kind, TokenType::LeftShiftEqual);
    assert_eq!(tokens.len(), 2); // LeftShiftEqual + EOF
}

#[test]
fn test_question_dot_vs_question() {
    let tokens = tokenize("?. ?").unwrap();
    assert_eq!(tokens[0].kind, TokenType::QuestionDot);
    assert_eq!(tokens[1].kind, TokenType::Question);
}

#[test]
fn test_hex_literal_value() {
    let tokens = tokenize("0xFF").unwrap();
    assert_eq!(tokens[0].kind, TokenType::IntLit(255));
}

#[test]
fn test_binary_literal_value() {
    let tokens = tokenize("0b1111").unwrap();
    assert_eq!(tokens[0].kind, TokenType::IntLit(15));
}

#[test]
fn test_float_suffix_f_gives_float_lit() {
    let tokens = tokenize("1.5f").unwrap();
    assert!(matches!(tokens[0].kind, TokenType::FloatLit(_)));
}

#[test]
fn test_double_without_suffix() {
    let tokens = tokenize("1.5").unwrap();
    assert!(matches!(tokens[0].kind, TokenType::DoubleLit(_)));
}

#[test]
fn test_verbatim_string_no_escape() {
    let input = r#"@"C:\Users\test""#;
    let tokens = tokenize(input).unwrap();
    assert!(
        matches!(tokens[0].kind, TokenType::VerbatimString(ref s) if s.contains('\\'))
    );
}

#[test]
fn test_multiline_string_interpolation() {
    let input = "$\"line1\nline2\"";
    let tokens = tokenize(input).unwrap();
    match &tokens[0].kind {
        TokenType::InterpolatedString(parts) => {
            let text: String = parts
                .iter()
                .filter_map(|p| {
                    if let InterpolationPart::Text(t) = p {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert!(text.contains('\n'));
        }
        _ => panic!("Expected InterpolatedString"),
    }
}
