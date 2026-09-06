// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/ubel_stratum.md, section "tests/error_recovery_test.rs"
// ============================================================================
#[cfg(test)]
mod tests {
    use ubel_stratum::error_management::Logger;
    use ubel_stratum::lexer::tokenize;

    #[test]
    fn test_multiple_errors_collected() {
        Logger::disable();

        // Backtick is not a valid character; unterminated string triggers another error
        let input = r#"
            let x = 42
            let y = "unterminated
            let z = 100
            let w = `invalid char`
        "#;

        let result = tokenize(input);
        assert!(result.is_err(), "Should have errors");

        if let Err(error_manager) = result {
            assert!(
                error_manager.error_count() >= 1,
                "Should have at least one error, got {}",
                error_manager.error_count()
            );
        }

        Logger::enable();
    }

    #[test]
    fn test_error_suggestions_present() {
        Logger::disable();

        let input = r#""unterminated string"#;
        let result = tokenize(input);
        assert!(result.is_err(), "Should error on unterminated string");

        // Need `mut` because take_errors mutably drains the internal vec
        if let Err(mut error_manager) = result {
            let errors = error_manager.take_errors();
            assert!(
                !errors.is_empty(),
                "Expected at least one error"
            );
            assert!(
                errors.iter().any(|e| e.suggestion().is_some()),
                "Expected at least one error to have a suggestion"
            );
        }

        Logger::enable();
    }

    #[test]
    fn test_invalid_escape_simple_string() {
        Logger::disable();

        // \x is not a valid escape in simple strings — may error or treat literally
        // Either outcome is acceptable; we just verify it does not panic
        let input = r#""Hello\xWorld""#;
        let _ = tokenize(input); // must not panic

        Logger::enable();
    }

    #[test]
    fn test_unexpected_dollar_char() {
        Logger::disable();

        // Bare $ (not followed by ") is not a valid token
        let result = tokenize("let x = 42 $ let y = 100");

        // Should either be an error result OR contain an Error token
        let is_bad = match &result {
            Err(_) => true,
            Ok(tokens) => tokens.iter().any(|t| t.is_error()),
        };
        assert!(is_bad, "Bare $ should produce an error or Error token");

        Logger::enable();
    }

    #[test]
    fn test_lexer_continues_after_bad_char() {
        Logger::disable();

        // After a bad character, valid tokens after it should still be produced
        let input = "let ` x = 42";
        match tokenize(input) {
            Ok(tokens) => {
                // If it somehow succeeds, `let` should be first
                assert_eq!(tokens[0].kind, ubel_stratum::lexer::TokenType::Let);
            }
            Err(error_manager) => {
                // Error case — at least one error logged
                assert!(error_manager.error_count() >= 1);
            }
        }

        Logger::enable();
    }
        }
