#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::{tokenize, TokenType, InterpolationPart};

    #[test]
    fn test_simple_interpolation() {
        let input = r#"$"Hello, {name}!""#;
        let tokens = tokenize(input).unwrap();

        match &tokens[0].kind {
            TokenType::InterpolatedString(parts) => {
                assert_eq!(parts.len(), 3);
                assert!(matches!(parts[0], InterpolationPart::Text(ref s) if s == "Hello, "));
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "name"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "id"));
                assert!(matches!(parts[2], InterpolationPart::Text(ref s) if s == ": "));
                assert!(matches!(parts[3], InterpolationPart::Expr(ref s) if s == "name"));
                assert!(matches!(parts[4], InterpolationPart::Text(ref s) if s == " ("));
                assert!(matches!(parts[5], InterpolationPart::Expr(ref s) if s == "email"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "x + y"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "user.getName()"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "arr[{idx}]"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "username"));
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
                assert!(matches!(parts[1], InterpolationPart::Expr(ref s) if s == "content"));
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