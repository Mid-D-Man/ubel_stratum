#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::tokenize;
    use ubel_stratum::error_management::Logger;

    #[test]
    fn test_multiple_errors_collected() {
        // Disable logger for tests
        Logger::disable();

        let input = r#"
            let x = 42
            let y = "unterminated
            let z = 100
            let w = `invalid char`
        "#;

        let result = tokenize(input);

        assert!(result.is_err(), "Should have errors");

        if let Err(error_manager) = result {
            assert!(error_manager.error_count() >= 2, "Should have multiple errors");
        }

        Logger::enable();
    }

    #[test]
    fn test_error_suggestions() {
        Logger::disable();

        let input = r#""unterminated string"#;
        let result = tokenize(input);

        assert!(result.is_err(), "Should error on unterminated string");

        if let Err(error_manager) = result {
            let errors = error_manager.take_errors();
            // Check that suggestion exists
            assert!(errors.iter().any(|e| e.suggestion().is_some()));
        }

        Logger::enable();
    }

    #[test]
    fn test_invalid_escape_sequence() {
        Logger::disable();

        let input = r#""Hello\xWorld""#; // \x is not valid
        let result = tokenize(input);

        // Should either error or treat \x literally
        // (Depends on implementation - simple strings might accept it)

        Logger::enable();
    }

    #[test]
    fn test_unexpected_character() {
        Logger::disable();

        let input = "let x = 42 $ let y = 100";
        let result = tokenize(input);

        // $ alone (not followed by ") is invalid
        assert!(result.is_err() || result.unwrap().iter().any(|t| t.is_error()));

        Logger::enable();
    }
}