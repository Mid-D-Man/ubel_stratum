#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::{tokenize, TokenType};

    #[test]
    fn test_keywords() {
        let input = "fn let mut const if elif else match where for in while loop break continue return";
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::Fn);
        assert_eq!(tokens[1].kind, TokenType::Let);
        assert_eq!(tokens[2].kind, TokenType::Mut);
        assert_eq!(tokens[3].kind, TokenType::Const);
        assert_eq!(tokens[4].kind, TokenType::If);
        assert_eq!(tokens[5].kind, TokenType::Elif);
        assert_eq!(tokens[6].kind, TokenType::Else);
        assert_eq!(tokens[7].kind, TokenType::Match);
        assert_eq!(tokens[8].kind, TokenType::Where);
        assert_eq!(tokens[9].kind, TokenType::For);
        assert_eq!(tokens[10].kind, TokenType::In);
        assert_eq!(tokens[11].kind, TokenType::While);
        assert_eq!(tokens[12].kind, TokenType::Loop);
        assert_eq!(tokens[13].kind, TokenType::Break);
        assert_eq!(tokens[14].kind, TokenType::Continue);
        assert_eq!(tokens[15].kind, TokenType::Return);
        assert_eq!(tokens[16].kind, TokenType::Eof);
    }

    #[test]
    fn test_operators() {
        let input = "+ - * / % & | ^ ~ << >> == != < > <= >= ! && || = += -= := => ?.";
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::Plus);
        assert_eq!(tokens[1].kind, TokenType::Minus);
        assert_eq!(tokens[2].kind, TokenType::Star);
        assert_eq!(tokens[3].kind, TokenType::Slash);
        assert_eq!(tokens[4].kind, TokenType::Percent);
        assert_eq!(tokens[5].kind, TokenType::Amp);
        assert_eq!(tokens[6].kind, TokenType::Pipe);
        assert_eq!(tokens[7].kind, TokenType::Caret);
        assert_eq!(tokens[8].kind, TokenType::Tilde);
        assert_eq!(tokens[9].kind, TokenType::LeftShift);
        assert_eq!(tokens[10].kind, TokenType::RightShift);
        assert_eq!(tokens[11].kind, TokenType::EqualEqual);
        assert_eq!(tokens[12].kind, TokenType::BangEqual);
        assert_eq!(tokens[13].kind, TokenType::Less);
        assert_eq!(tokens[14].kind, TokenType::Greater);
        assert_eq!(tokens[15].kind, TokenType::LessEqual);
        assert_eq!(tokens[16].kind, TokenType::GreaterEqual);
        assert_eq!(tokens[17].kind, TokenType::Bang);
        assert_eq!(tokens[18].kind, TokenType::AmpAmp);
        assert_eq!(tokens[19].kind, TokenType::PipePipe);
        assert_eq!(tokens[20].kind, TokenType::Equal);
        assert_eq!(tokens[21].kind, TokenType::PlusEqual);
        assert_eq!(tokens[22].kind, TokenType::MinusEqual);
        assert_eq!(tokens[23].kind, TokenType::ColonEqual);
        assert_eq!(tokens[24].kind, TokenType::FatArrow);
        assert_eq!(tokens[25].kind, TokenType::QuestionDot);
    }

    #[test]
    fn test_delimiters() {
        let input = "( ) { } [ ] , . : ; @";
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::LeftParen);
        assert_eq!(tokens[1].kind, TokenType::RightParen);
        assert_eq!(tokens[2].kind, TokenType::LeftBrace);
        assert_eq!(tokens[3].kind, TokenType::RightBrace);
        assert_eq!(tokens[4].kind, TokenType::LeftBracket);
        assert_eq!(tokens[5].kind, TokenType::RightBracket);
        assert_eq!(tokens[6].kind, TokenType::Comma);
        assert_eq!(tokens[7].kind, TokenType::Dot);
        assert_eq!(tokens[8].kind, TokenType::Colon);
        assert_eq!(tokens[9].kind, TokenType::Semicolon);
        assert_eq!(tokens[10].kind, TokenType::At);
    }

    #[test]
    fn test_identifiers() {
        let input = "foo bar_baz _test123 CamelCase";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::Ident(ref s) if s == "foo"));
        assert!(matches!(tokens[1].kind, TokenType::Ident(ref s) if s == "bar_baz"));
        assert!(matches!(tokens[2].kind, TokenType::Ident(ref s) if s == "_test123"));
        assert!(matches!(tokens[3].kind, TokenType::Ident(ref s) if s == "CamelCase"));
    }

    #[test]
    fn test_integer_literals() {
        let input = "42 1000 0xFF 0b1010 1_000_000";
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::IntLit(42));
        assert_eq!(tokens[1].kind, TokenType::IntLit(1000));
        assert_eq!(tokens[2].kind, TokenType::IntLit(255)); // 0xFF
        assert_eq!(tokens[3].kind, TokenType::IntLit(10)); // 0b1010
        assert_eq!(tokens[4].kind, TokenType::IntLit(1_000_000));
    }

    #[test]
    fn test_float_literals() {
        let input = "3.14 3.14f 1e10 1e10f 2.5e-3";
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::DoubleLit(f) if (f - 3.14).abs() < 0.001));
        assert!(matches!(tokens[1].kind, TokenType::FloatLit(f) if (f - 3.14).abs() < 0.001));
        assert!(matches!(tokens[2].kind, TokenType::DoubleLit(f) if (f - 1e10).abs() < 1.0));
        assert!(matches!(tokens[3].kind, TokenType::FloatLit(f) if (f - 1e10).abs() < 1.0));
        assert!(matches!(tokens[4].kind, TokenType::DoubleLit(f) if (f - 2.5e-3).abs() < 0.0001));
    }

    #[test]
    fn test_simple_strings() {
        let input = r#""hello" "world\n" "with \"quotes\"" "#;
        let tokens = tokenize(input).unwrap();

        assert!(matches!(tokens[0].kind, TokenType::StringLit(ref s) if s == "hello"));
        assert!(matches!(tokens[1].kind, TokenType::StringLit(ref s) if s == "world\n"));
        assert!(matches!(tokens[2].kind, TokenType::StringLit(ref s) if s == "with \"quotes\""));
    }

    #[test]
    fn test_char_literals() {
        let input = r#"'a' 'Z' '\n' '\t' '\\' '\''"#;
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::CharLit('a'));
        assert_eq!(tokens[1].kind, TokenType::CharLit('Z'));
        assert_eq!(tokens[2].kind, TokenType::CharLit('\n'));
        assert_eq!(tokens[3].kind, TokenType::CharLit('\t'));
        assert_eq!(tokens[4].kind, TokenType::CharLit('\\'));
        assert_eq!(tokens[5].kind, TokenType::CharLit('\''));
    }

    #[test]
    fn test_booleans_and_null() {
        let input = "true false null";
        let tokens = tokenize(input).unwrap();

        assert_eq!(tokens[0].kind, TokenType::True);
        assert_eq!(tokens[1].kind, TokenType::False);
        assert_eq!(tokens[2].kind, TokenType::Null);
    }
}