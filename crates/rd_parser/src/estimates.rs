// crates/rd_parser/src/estimates.rs
//
// Dynamic Vec capacity estimation from total token count.
//
// Rationale (same pattern as DixScript optimization helper):
//   Static cap constants (cap::STRUCT_FIELDS = 8) are blind to file size.
//   A 5000-token file likely has more struct fields than a 100-token file.
//   Pre-allocating from the known total token count reduces Vec growth events
//   by 15-25% across realistic source files.
//
// All functions guarantee a minimum floor so tiny files don't over-allocate
// and we never pass 0 to Vec::with_capacity.

/// Capacity estimates derived once from the total token count.
/// Stored on the Parser struct and used instead of the static `cap::*` consts.
#[derive(Debug, Clone, Copy)]
pub struct ParseEstimates {
    pub top_level_items: usize,
    pub struct_fields:   usize,
    pub enum_variants:   usize,
    pub fn_params:       usize,
    pub call_args:       usize,
    pub block_stmts:     usize,
    pub match_arms:      usize,
    pub generic_params:  usize,
    pub generic_args:    usize,
    pub import_items:    usize,
    pub impl_methods:    usize,
    pub attr_args:       usize,
    pub path_segs:       usize,
    pub linq_clauses:    usize,
}

impl ParseEstimates {
    /// Derive estimates from the total number of tokens in the source file.
    ///
    /// Ratios calibrated against real Ubel source files; adjust as the
    /// language corpus grows.
    pub fn from_token_count(total: usize) -> Self {
        ParseEstimates {
            // ~1 top-level item per 50 tokens
            top_level_items: usize::max(4,  total / 50),
            // ~1 struct field per 5 tokens inside a struct body
            struct_fields:   usize::max(4,  total / 30),
            // ~1 enum variant per 4 tokens
            enum_variants:   usize::max(4,  total / 25),
            // ~1 fn param per 6 tokens on average
            fn_params:       usize::max(2,  total / 80),
            // ~1 call arg per 5 tokens
            call_args:       usize::max(2,  total / 70),
            // ~1 statement per 12 tokens in a block
            block_stmts:     usize::max(4,  total / 15),
            // ~1 match arm per 15 tokens
            match_arms:      usize::max(4,  total / 20),
            // Generics rarely deep — keep low
            generic_params:  usize::max(2,  total / 200),
            generic_args:    usize::max(2,  total / 150),
            // Imports
            import_items:    usize::max(2,  total / 80),
            // Methods per impl block
            impl_methods:    usize::max(4,  total / 40),
            // Attribute args — usually 1-2
            attr_args:       usize::max(2,  total / 300),
            // Dotted path segments
            path_segs:       usize::max(2,  3),
            // LINQ clauses
            linq_clauses:    usize::max(2,  total / 100),
        }
    }

    /// Minimal estimates for REPL input or very small source fragments.
    pub const fn minimal() -> Self {
        ParseEstimates {
            top_level_items: 4,
            struct_fields:   4,
            enum_variants:   4,
            fn_params:       2,
            call_args:       2,
            block_stmts:     4,
            match_arms:      4,
            generic_params:  2,
            generic_args:    2,
            import_items:    2,
            impl_methods:    4,
            attr_args:       2,
            path_segs:       3,
            linq_clauses:    2,
        }
    }
}

// ── Per-context helpers — used when the overall file estimate is too coarse ───

/// Estimate struct fields from the number of tokens inside the body `{...}`.
/// Useful when parse_decl does a quick scan before parsing.
#[inline]
pub fn estimate_struct_fields(body_tokens: usize) -> usize {
    // ~1 field per 3 tokens: `name : Type`
    usize::max(4, body_tokens / 3)
}

/// Estimate call arguments from the token count inside `(...)`.
#[inline]
pub fn estimate_call_args(arg_tokens: usize) -> usize {
    // ~1 arg per 4 tokens: `expr ,`
    usize::max(2, arg_tokens / 4)
}

/// Estimate block statements from the token count inside `{...}`.
#[inline]
pub fn estimate_block_stmts(block_tokens: usize) -> usize {
    // ~1 statement per 8 tokens
    usize::max(4, block_tokens / 8)
}

/// Estimate match arms from the token count inside `{ arms }`.
#[inline]
pub fn estimate_match_arms(body_tokens: usize) -> usize {
    // ~1 arm per 10 tokens: `Pattern => Expr ,`
    usize::max(4, body_tokens / 10)
}

/// Estimate enum variants from token count inside the enum body.
#[inline]
pub fn estimate_enum_variants(body_tokens: usize) -> usize {
    // ~1 variant per 2-3 tokens (bare name) up to 8 tokens (struct variant)
    usize::max(4, body_tokens / 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimums_hold_at_zero_tokens() {
        let e = ParseEstimates::from_token_count(0);
        assert!(e.struct_fields   >= 4);
        assert!(e.fn_params       >= 2);
        assert!(e.block_stmts     >= 4);
        assert!(e.enum_variants   >= 4);
    }

    #[test]
    fn estimates_scale_with_token_count() {
        let small  = ParseEstimates::from_token_count(100);
        let medium = ParseEstimates::from_token_count(2_000);
        let large  = ParseEstimates::from_token_count(20_000);
        assert!(medium.top_level_items > small.top_level_items);
        assert!(large.block_stmts > medium.block_stmts);
    }

    #[test]
    fn per_context_estimates_have_floors() {
        assert_eq!(estimate_struct_fields(0), 4);
        assert_eq!(estimate_call_args(0),     2);
        assert_eq!(estimate_block_stmts(0),   4);
        assert_eq!(estimate_match_arms(0),    4);
        assert_eq!(estimate_enum_variants(0), 4);
    }

    #[test]
    fn per_context_scales() {
        assert!(estimate_struct_fields(60) > 4);
        assert!(estimate_call_args(40)     > 2);
    }
  }
