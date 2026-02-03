#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::{tokenize, TokenType};

    #[test]
    fn test_line_comment() {
        let input = r#"
            let x = 42  // This is a comment
            let y = 100
        "#;
        let tokens = tokenize(input).unwrap();

        // Line comments should be stripped
        assert!(matches!(tokens[0].kind, TokenType::Let));
        assert!(matches!(tokens[1].kind, TokenType::Ident(_)));
        assert!(matches!(tokens[2].kind, TokenType::Equal));
        assert!(matches!(tokens[3].kind, TokenType::IntLit(42)));
        assert!(matches!(tokens[4].kind, TokenType::Let));
    }

    #[test]
    fn test_block_comment() {
        let input = r#"
            let x = 42
            /* This is a
               multi-line comment */
            let y = 100
        "#;
        let tokens = tokenize(input).unwrap();

        // Block comments should be stripped
        assert!(matches!(tokens[0].kind, TokenType::Let));
        assert!(matches!(tokens[3].kind, TokenType::IntLit(42)));
        assert!(matches!(tokens[4].kind, TokenType::Let));
    }

    #[test]
    fn test_nested_block_comments() {
        let input = r#"
            let x = 42
            /* Outer /* Inner */ Still outer */
            let y = 100
        "#;
        let tokens = tokenize(input).unwrap();

        // Nested comments should work
        assert!(matches!(tokens[0].kind, TokenType::Let));
        assert!(matches!(tokens[3].kind, TokenType::IntLit(42)));
        assert!(matches!(tokens[4].kind, TokenType::Let));
    }

    #[test]
    fn test_doc_comment_star() {
        let input = r#"
            /** This is a doc comment */
            fn foo() {}
        "#;
        let tokens = tokenize(input).unwrap();

        // Doc comments SHOULD be kept
        assert!(matches!(tokens[0].kind, TokenType::DocComment(_)));
        assert!(matches!(tokens[1].kind, TokenType::Fn));
    }

    #[test]
    fn test_doc_comment_bang() {
        let input = r#"
            /*!
             * Module documentation
             * Multiple lines
             */
            fn foo() {}
        "#;
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::DocComment(_)));
        assert!(matches!(tokens[1].kind, TokenType::Fn));
    }

    #[test]
    fn test_unterminated_block_comment() {
        let input = r#"
            let x = 42
            /* This comment never ends
        "#;
        let result = tokenize(input);

        assert!(result.is_err(), "Should error on unterminated block comment");
    }

    #[test]
    fn test_comment_in_string_not_treated_as_comment() {
        let input = r#""This is // not a comment""#;
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::StringLit(ref s)
            if s == "This is // not a comment"));
    }
}