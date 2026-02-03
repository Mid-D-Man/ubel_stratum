#[cfg(test)]
mod tests {
    use ubel_stratum::lexer::{tokenize, TokenType};
    
    #[test]
    fn test_all_keywords() {
        let input = "fn let mut const if elif else";
        let tokens = tokenize(input).unwrap();
        
        assert_eq!(tokens[0].kind, TokenType::Fn);
        assert_eq!(tokens[1].kind, TokenType::Let);
        assert_eq!(tokens[2].kind, TokenType::Mut);
    }
}
