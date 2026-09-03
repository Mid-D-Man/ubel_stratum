# ubel_stratum_rd

## Overview

Recursive descent parser crate for Ubel Stratum. Lexes and parses
source text into the AST types defined in `ubel_stratum`. See
`PARSER_RULES.md` for the architecture (recursive descent for
declarations and statements, Pratt parsing for expressions, targeted
memoization for the few genuinely ambiguous cases).

## Modules

### `parsers/parse_pattern.rs`

**What it does:** Parses match arm patterns and struct/array destructure
patterns.

**Tests:** see `tests/fixtures/ok_wildcard_and_discard_isolated.ubl`,
`tests/fixtures/ok_callback_registry_combined.ubl`, and
`tests/fixtures/err_underscore_in_expression_position.ubl`.

### `parsers/parse_decl.rs`

**What it does:** Parses top-level and member declarations: functions,
methods, parameters, structs, enums, traits, impls.

**Decisions:**
- Added a `TokenType::Underscore` arm to `parse_param` for `_: Type`
  parameters, producing `ParamKind::Discard`.

**Tests:** see `tests/fixtures/ok_wildcard_and_discard_isolated.ubl` and
`tests/fixtures/ok_callback_registry_combined.ubl`.

## CI and Workflows

- `.github/workflows/ci-check.yml`, "Ubel Stratum, Fast Compile Check":
  runs on every push and pull request to `master`/`main`, covers this
  crate along with `ubel_stratum`.
- `.github/workflows/parser-crate-migrate.yml`: migration workflow for
  this crate's split from the original monolithic parser.

## Fixes and Problems

### `parsers/parse_pattern.rs`

- Both of this file's wildcard-recognizing arms, one for match-arm
  patterns and one for struct-destructure fields, checked for
  `TokenType::Ident(n) if n == "_"`. The lexer tokenizes a bare `_` as
  its own token, `TokenType::Underscore`, specifically so it is never
  `Ident("_")`. That meant neither arm could ever fire for a real `_`
  in source text: writing `_ => ...` in a match produced a parse error
  instead of a wildcard match, even though `PatternKind::Wildcard`
  itself was already fully implemented and correct everywhere it was
  consumed (name resolution, `PatternCoverage::CatchAll` in type
  inference, the interpreter's pattern matcher). Confirmed via a real
  `_ => ...` match arm before fixing anything: the parser's own error
  message listed `'_'` as an expected token right next to the
  `Underscore` token it had actually received. Fixed by checking
  `TokenType::Underscore` directly in both places instead.
