//! Diagnostic formatting and suggestions

use crate::lexer::Span;
use crate::error_management::error_types::LexicalError;

pub struct DiagnosticFormatter;

impl DiagnosticFormatter {
    pub fn format_lexical_error(error: &LexicalError, source: &str) -> String {
        let span = error.span();
        let message = error.message();
        let suggestion = error.suggestion();

        let mut output = String::new();

        // Extract line
        let lines: Vec<&str> = source.lines().collect();
        let line_text = if span.line > 0 && span.line <= lines.len() {
            lines[span.line - 1]
        } else {
            ""
        };

        // Format error
        output.push_str(&format!("\x1b[31merror:\x1b[0m {}\n", message));
        output.push_str(&format!("  \x1b[36m--> {}:{}\x1b[0m\n", span.line, span.column));
        output.push_str("   |\n");
        output.push_str(&format!("{:3} | {}\n", span.line, line_text));
        output.push_str(&format!("   | {}{}\n",
                                 " ".repeat(span.column.saturating_sub(1)),
                                 "\x1b[31m^\x1b[0m"
        ));

        if let Some(suggest) = suggestion {
            output.push_str(&format!("   \x1b[33m= help:\x1b[0m {}\n", suggest));
        }

        output
    }
}