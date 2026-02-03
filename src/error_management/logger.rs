//! Custom logger with formatting and enable/disable

use std::sync::atomic::{AtomicBool, Ordering};
use std::fmt;

static LOGGER_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

pub struct Logger;

impl Logger {
    pub fn enable() {
        LOGGER_ENABLED.store(true, Ordering::SeqCst);
    }

    pub fn disable() {
        LOGGER_ENABLED.store(false, Ordering::SeqCst);
    }

    pub fn is_enabled() -> bool {
        LOGGER_ENABLED.load(Ordering::SeqCst)
    }

    pub fn debug(message: &str) {
        Self::log(LogLevel::Debug, message);
    }

    pub fn info(message: &str) {
        Self::log(LogLevel::Info, message);
    }

    pub fn warning(message: &str) {
        Self::log(LogLevel::Warning, message);
    }

    pub fn error(message: &str) {
        Self::log(LogLevel::Error, message);
    }

    fn log(level: LogLevel, message: &str) {
        if !Self::is_enabled() {
            return;
        }

        let (prefix, color) = match level {
            LogLevel::Debug => ("DEBUG", "\x1b[36m"),    // Cyan
            LogLevel::Info => ("INFO", "\x1b[32m"),      // Green
            LogLevel::Warning => ("WARN", "\x1b[33m"),   // Yellow
            LogLevel::Error => ("ERROR", "\x1b[31m"),    // Red
        };

        eprintln!("{}[{}]\x1b[0m {}", color, prefix, message);
    }

    pub fn formatted_error(error: &impl fmt::Display, span: &crate::lexer::Span, source: &str) {
        if !Self::is_enabled() {
            return;
        }

        // Extract source line
        let lines: Vec<&str> = source.lines().collect();
        let line_text = if span.line > 0 && span.line <= lines.len() {
            lines[span.line - 1]
        } else {
            ""
        };

        eprintln!("\x1b[31m[ERROR]\x1b[0m {}", error);
        eprintln!("  \x1b[36m--> {}:{}\x1b[0m", span.line, span.column);
        eprintln!("   |");
        eprintln!("{:3} | {}", span.line, line_text);
        eprintln!("   | {}{}",
                  " ".repeat(span.column.saturating_sub(1)),
                  "\x1b[31m^\x1b[0m"
        );
    }
}