//! Error management system

pub mod error_manager;
pub mod logger;
pub mod error_types;
pub mod diagnostics;

pub use error_manager::ErrorManager;
pub use logger::Logger;
pub use diagnostics::DiagnosticFormatter;