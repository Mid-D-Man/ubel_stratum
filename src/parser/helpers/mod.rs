














// src/parser/helpers/mod.rs
//! Arena-aware builder helpers used by grammar actions.
//!
//! Keeping complex construction logic here keeps grammar actions readable
//! and makes each builder independently testable.
















pub mod decl;
pub mod expr;
pub mod pat;
pub mod stmt;
pub mod ty;
















// Convenience re-export so grammar.lalrpop can write `helpers::*`
pub use decl::*;
pub use expr::*;
pub use pat::*;
pub use stmt::*;
pub use ty::*;
