// src/error_management/error_types/name_error.rs
//! Errors produced during the name-resolution pass.

use crate::lexer::Span;
use std::fmt;

/// Every error that can be raised while resolving names to definitions.
#[derive(Debug, Clone)]
pub enum NameError {
    /// An identifier was used but never defined in any reachable scope.
    UndefinedName {
        name: String,
        span: Span,
        /// If we found something close, suggest it.
        did_you_mean: Option<String>,
    },

    /// The same name was declared twice in the same scope.
    DuplicateDefinition {
        name:          String,
        first_defined: Span,
        redefined_at:  Span,
    },

    /// `summon` (or `from ... summon`) referred to a path that does not exist.
    UnresolvedImport {
        path: String,
        span: Span,
    },

    /// A dotted path like `std.io.File` was partially resolved but the
    /// final segment was not found inside the resolved module.
    UnresolvedPathSegment {
        full_path:        String,
        unresolved_at:    String,
        resolved_so_far:  String,
        span:             Span,
    },

    /// `self` was used outside of a method body.
    SelfOutsideMethod {
        span: Span,
    },

    /// A type parameter (generic) name was referenced but not declared.
    UnresolvedTypeParam {
        name: String,
        span: Span,
    },
}

impl NameError {
    pub fn span(&self) -> Span {
        match self {
            NameError::UndefinedName          { span, .. } => *span,
            NameError::DuplicateDefinition    { redefined_at, .. } => *redefined_at,
            NameError::UnresolvedImport       { span, .. } => *span,
            NameError::UnresolvedPathSegment  { span, .. } => *span,
            NameError::SelfOutsideMethod      { span }     => *span,
            NameError::UnresolvedTypeParam    { span, .. } => *span,
        }
    }

    pub fn message(&self) -> String {
        match self {
            NameError::UndefinedName { name, .. } =>
                format!("Undefined name `{}`", name),

            NameError::DuplicateDefinition { name, .. } =>
                format!("`{}` is already defined in this scope", name),

            NameError::UnresolvedImport { path, .. } =>
                format!("Cannot resolve import path `{}`", path),

            NameError::UnresolvedPathSegment { full_path, unresolved_at, resolved_so_far, .. } =>
                format!(
                    "No member `{}` in `{}` (while resolving `{}`)",
                    unresolved_at, resolved_so_far, full_path
                ),

            NameError::SelfOutsideMethod { .. } =>
                "`self` can only be used inside a method body".to_string(),

            NameError::UnresolvedTypeParam { name, .. } =>
                format!("Unknown type parameter `{}`", name),
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            NameError::UndefinedName { did_you_mean: Some(s), .. } =>
                Some(format!("Did you mean `{}`?", s)),

            NameError::DuplicateDefinition { first_defined, .. } =>
                Some(format!(
                    "First defined at line {}, column {}",
                    first_defined.line, first_defined.column
                )),

            NameError::SelfOutsideMethod { .. } =>
                Some("Move this code into a method that takes `self` as a parameter".to_string()),

            _ => None,
        }
    }
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for NameError {}
