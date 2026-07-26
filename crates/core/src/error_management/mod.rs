//! Error management system

pub mod error_manager;
pub mod logger;
pub mod errors;
pub mod render;

pub use error_manager::ErrorManager;
pub use logger::Logger;
pub use render::{Diagnosable, Diagnostic, render, render_all};