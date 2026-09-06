// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/ubel_stratum.md, section "tests/string_interpolation_test.rs"
// ============================================================================
#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::{tokenize, TokenType, InterpolationPart};

    /// `InterpolationPart::Expr` holds a pre-tokenized `Vec<Token>` (the
    /// hole gets lexed once, up front, rather than kept as raw source
    /// text for a later re-lex), so these tests check the resulting
    /// token *kinds* directly rather than a source string. Every hole's
    /// token vec ends with its own `Eof`, confirmed empirically before
    /// writing these assertions, not assumed.
    fn expr_kinds(part: &InterpolationPart) -> Vec<TokenType> {
        match part {
            InterpolationPart::Expr(toks) => toks.iter().map(|t| t.kind.clone()).collect(),
            InterpolationPart::Text(_) => panic!("expected an Expr part, got Text"),
        }
    }

    #[test]
    fn test_simple_interpolation() {
        let input = r#"$"Hello, {name}!""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == "Hello, "));
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("name".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[2], InterpolationPart::Text(ref s) if s == "!"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_multiple_interpolations() {
        let input = r#"$"User {id}: {name} ({email})""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 7);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == "User "));
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("id".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[2], InterpolationPart::Text(ref s) if s == ": "));
                assert_eq!(expr_kinds(&parts[3]), vec![
                    TokenType::Ident("name".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[4], InterpolationPart::Text(ref s) if s == " ("));
                assert_eq!(expr_kinds(&parts[5]), vec![
                    TokenType::Ident("email".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[6], InterpolationPart::Text(ref s) if s == ")"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_interpolation_with_expression() {
        let input = r#"$"Result: {x + y}""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == "Result: "));
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("x".to_string()), TokenType::Plus,
                    TokenType::Ident("y".to_string()), TokenType::Eof,
                ]);
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_interpolation_with_method_call() {
        let input = r#"$"Name: {user.getName()}""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("user".to_string()), TokenType::Dot,
                    TokenType::Ident("getName".to_string()),
                    TokenType::LeftParen, TokenType::RightParen, TokenType::Eof,
                ]);
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_interpolation_with_nested_braces() {
        let input = r#"$"Array: {arr[{idx}]}""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 2);
                // Brace-depth counting inside a hole: the inner `{idx}`
                // must not be mistaken for the hole's own closing brace.
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("arr".to_string()), TokenType::LeftBracket,
                    TokenType::LeftBrace, TokenType::Ident("idx".to_string()),
                    TokenType::RightBrace, TokenType::RightBracket, TokenType::Eof,
                ]);
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_verbatim_string() {
        let input = r#"@"C:\Users\Alice\Documents""#;
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::VerbatimString(ref s)
            if s == r"C:\Users\Alice\Documents"));
    }

    #[test]
    fn test_verbatim_string_with_doubled_quotes() {
        let input = r#"@"She said ""Hello""!""#;
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::VerbatimString(ref s)
            if s == r#"She said "Hello"!"#));
    }

    #[test]
    fn test_interpolated_verbatim_string() {
        let input = r#"$@"C:\Users\{username}\Documents""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == r"C:\Users\"));
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("username".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[2], InterpolationPart::Text(ref s) if s == r"\Documents"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_escape_sequences_in_interpolation() {
        let input = r#"$"Line 1\n{content}\nLine 3""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == "Line 1\n"));
                assert_eq!(expr_kinds(&parts[1]), vec![
                    TokenType::Ident("content".to_string()), TokenType::Eof,
                ]);
                assert!(matches!(parts[2], InterpolationPart::Text(ref s) if s == "\nLine 3"));
            }
            _ => panic!("Expected interpolated string"),
        }
    }

    #[test]
    fn test_unterminated_interpolated_string() {
        let input = r#"$"Hello, {name}"#; // Missing closing "
        let result = tokenize(input);

        assert!(result.is_err(), "Should error on unterminated string");
    }

    #[test]
    fn test_unclosed_interpolation_expr() {
        let input = r#"$"Hello, {name""#; // Missing }
        let result = tokenize(input);

        assert!(result.is_err(), "Should error on unclosed interpolation");
    }
}