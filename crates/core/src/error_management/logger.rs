//! Custom logger with formatting and enable/disable

use std::sync::atomic::{AtomicBool, Ordering};

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
}