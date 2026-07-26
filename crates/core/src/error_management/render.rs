// src/error_management/render.rs
//! The single, unified diagnostic renderer for the whole compiler.
//!
//! Before this file, there were THREE separate, drifting implementations
//! of "print an error nicely": `diagnostics.rs::DiagnosticFormatter`
//! (dead code, never called), `logger.rs::Logger::formatted_error` (also
//! dead code, never called), and `error_manager.rs::report_section`'s own
//! inline copy (the only one actually wired up, and only into the legacy
//! `crates/parser` CLI). All three only drew a single `^` at the start
//! column, regardless of how long the offending span actually was.
//!
//! This is the replacement. It implements the format specified in
//! docs/DIAGNOSTICS_RULES.md: a full-width underline under the exact
//! span (not one character), a stable error code, optional secondary
//! spans (e.g. "first defined here"), and a `= help:` suggestion line.
//! Every other renderer in the crate now delegates here — see
//! `error_manager.rs::report_section` and
//! `crates/rd_parser/examples/diagnose.rs`.
//!
//! # Fold markers
//!
//! Every fold region has an explicit, symmetric open AND close marker —
//! nothing is inferred from an implicit "this line's shape means the
//! region started here." That's deliberate: a dumb line-scanner (a
//! regex, an editor extension, grep) can then carve up a rendered
//! diagnostic, at ANY nesting depth, by matching open/close pairs alone,
//! with no indentation-sniffing and no real parser — this matters today
//! because `diagnose.rs`'s output is plain captured text, and matters
//! later because it's the same shape an LSP client will eventually want
//! to fold in an editor:
//!
//!   - **`>>>` / `<<<`** — wraps the WHOLE diagnostic. `>>>` starts the
//!     `error[CODE]: message` line itself; `<<<` is the diagnostic's
//!     last line, alone.
//!   - **`~>` / `<~`** — wraps each individual *supplementary* block: a
//!     `note` (points at a secondary span, e.g. "first defined here")
//!     or a `help` (a plain-text suggestion, no span). Every `~>` has
//!     exactly one matching `<~` closing it, even a one-line `help`
//!     with no nested span block of its own — no "sometimes symmetric"
//!     special case to remember.
//!
//! The one thing that's deliberately NOT wrapped in its own marker pair
//! is the primary span block (`-->`/`|`/`^^^^`, right after the `>>>
//! error[...]` line) — that's the one fact a fold-aware reader should
//! never be able to collapse away, only the `~>`/`<~` "extra context"
//! blocks are meant to fold independently of it and of each other.
//!
//! When real LSP support lands, `Diagnostic` here maps close to 1:1
//! onto LSP's own `Diagnostic` type (`code`, `message`, `range` from
//! `primary_span`, `relatedInformation` from `secondary`) — these text
//! markers are the interim, file-based version of the same split.
//!
//! # Plain text by design
//!
//! This module never emits ANSI escape codes. `diagnose.rs`'s output is
//! always captured to a file and re-displayed inside an HTML `<pre>`
//! block by the pipeline dashboard (see
//! `scripts/build_dashboard_report.py`), so embedded escape codes would
//! show up as literal garbage there, not color. `error_manager.rs`'s
//! `report_all` (the legacy `crates/parser` CLI's error path) prints
//! this same plain output too, for the same reason plain markdown is
//! easier to trust than clever formatting: one renderer, one behavior,
//! everywhere it's called from. If a genuinely interactive terminal
//! consumer shows up later (a REPL), it should colorize by wrapping
//! this module's plain output, not by teaching `render()` two modes.

use crate::lexer::Span;

/// A fully-resolved, phase-agnostic error ready to print. Every error
/// type in `errors/` converts to this via the `Diagnosable` trait
/// below — `render()` itself never matches on `LexicalError` /
/// `ParseError` / `NameError` / `TypeError` / `TierError` directly.
pub struct Diagnostic {
    pub code:          &'static str,
    pub message:       String,
    pub primary_span:  Span,
    /// Short text placed after the underline itself, e.g. "not found in
    /// this scope". Optional — most errors are clear from `message`
    /// alone and don't need a second, shorter restatement.
    pub primary_label: Option<String>,
    /// Other locations worth showing, each with its own short label —
    /// e.g. `DuplicateDefinition`'s "first defined here".
    pub secondary:     Vec<(Span, String)>,
    pub suggestion:    Option<String>,
}

/// Implemented by every error enum in `errors/`. `span()`,
/// `message()`, and `suggestion()` already existed on all of them
/// before this module; this trait just adds `code()` (required) and
/// two optional hooks with harmless defaults, then gives every
/// implementor `to_diagnostic()` for free.
pub trait Diagnosable {
    /// Stable, greppable identifier — see docs/DIAGNOSTICS_RULES.md
    /// "Error Code Registry" for the full table and the numbering
    /// scheme (`LEX-0xx` / `PARSE-0xx` / `NAME-0xx` / `TYPE-1xx` for
    /// ordinary type errors / `TIER-0xx` for tier-and-arena errors,
    /// physically split out of `TypeError` into their own `TierError`
    /// enum — see docs/DIAGNOSTICS_RULES.md §9).
    fn code(&self) -> &'static str;
    fn span(&self) -> Span;
    fn message(&self) -> String;
    fn suggestion(&self) -> Option<String>;

    /// Text shown right after the underline. Default: none.
    fn primary_label(&self) -> Option<String> { None }

    /// Other spans worth showing alongside the primary one. Default:
    /// none. `NameError::DuplicateDefinition` is the motivating case —
    /// see its override for the pattern to follow for any future error
    /// that references an earlier location.
    fn secondary_spans(&self) -> Vec<(Span, String)> { Vec::new() }

    fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic {
            code:          self.code(),
            message:       self.message(),
            primary_span:  self.span(),
            primary_label: self.primary_label(),
            secondary:     self.secondary_spans(),
            suggestion:    self.suggestion(),
        }
    }
}

/// Render one diagnostic against `source`, rustc-style, as plain text
/// (no ANSI). See docs/DIAGNOSTICS_RULES.md for a worked example of
/// this exact output, and the module doc above for what `~>` and `<<<`
/// are for.
pub fn render(diag: &Diagnostic, source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = String::new();

    out.push_str(&format!(
        ">>> error[{}]: {}\n",
        diag.code, diag.message
    ));
    render_span_block(
        &mut out,
        &lines,
        diag.primary_span,
        diag.primary_label.as_deref(),
        2,
    );

    for (span, label) in &diag.secondary {
        out.push_str("  ~> note: ");
        out.push_str(label);
        out.push('\n');
        render_span_block(&mut out, &lines, *span, None, 4);
        out.push_str("  <~\n");
    }

    if let Some(suggestion) = &diag.suggestion {
        out.push_str("  ~> help: ");
        out.push_str(suggestion);
        out.push('\n');
        out.push_str("  <~\n");
    }

    out.push_str("<<<\n");
    out
}

/// Render every diagnostic in `diags`, each ending with its own `<<<`
/// fold-close marker, separated by a blank line for human readability.
pub fn render_all(diags: &[Diagnostic], source: &str) -> String {
    diags
        .iter()
        .map(|d| render(d, source))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One `--> file... / gutter / source line / underline` block, shared
/// by both the primary span and every secondary span so they look
/// identical apart from `extra_indent` (0 for the primary span, 2 for
/// a secondary one, nesting it visually under its `~> note:` line).
/// Gutter width always comes from the line number's own digit count,
/// never guessed independently of `extra_indent` — the two were passed
/// as separately-hand-picked strings in an earlier version of this
/// function and drifted out of alignment for anything but 2-digit line
/// numbers; deriving both from one `extra_indent` makes that class of
/// bug impossible.
fn render_span_block(
    out:          &mut String,
    lines:        &[&str],
    span:         Span,
    label:        Option<&str>,
    extra_indent: usize,
) {
    let lead = " ".repeat(extra_indent);

    out.push_str(&lead);
    out.push_str(&format!("--> {}:{}\n", span.line, span.column));

    let line_text = if span.line > 0 && span.line <= lines.len() {
        lines[span.line - 1]
    } else {
        ""
    };

    // Gutter width follows the line number's own width so e.g. line 104
    // doesn't collide with the " | " separator — matches rustc.
    let gutter_width = span.line.to_string().len().max(1);
    let gutter_pad    = " ".repeat(gutter_width);

    out.push_str(&format!("{}{} |\n", lead, gutter_pad));
    out.push_str(&format!(
        "{}{:>width$} | {}\n",
        lead, span.line, line_text,
        width = gutter_width
    ));

    // Underline width is the span's real byte length, clamped so it
    // never runs past the end of the actual source line — a span whose
    // `end` was computed against a *different* line (Span has no
    // end_line/end_column field yet — see docs/DIAGNOSTICS_RULES.md
    // "Known limitation: multi-line spans") would otherwise draw a
    // wildly-too-long underline instead of visibly stopping short.
    let start_col   = span.column.saturating_sub(1);
    let raw_width   = span.len().max(1);
    let max_width   = line_text.chars().count().saturating_sub(start_col).max(1);
    let underline_w = raw_width.min(max_width);

    out.push_str(&format!(
        "{}{} | {}{}",
        lead,
        gutter_pad,
        " ".repeat(start_col),
        "^".repeat(underline_w),
    ));
    if let Some(label) = label {
        out.push(' ');
        out.push_str(label);
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize, line: usize, column: usize) -> Span {
        Span { start, end, line, column }
    }

    #[test]
    fn full_shape_with_secondary_and_help() {
        let source = "fn f() int {\n    let x = 1\n    return x\n}\n";
        let diag = Diagnostic {
            code:          "TYPE-101",
            message:       "type mismatch: expected `int`, found `string`".to_string(),
            primary_span:  span(0, 0, 3, 12),
            primary_label: Some("found here".to_string()),
            secondary:     vec![(span(0, 0, 1, 8), "expected type was established here".to_string())],
            suggestion:    Some("change the return type or the returned value".to_string()),
        };

        let out = render(&diag, source);

        // Anchors a fold-aware tool needs, exactly as documented in the
        // module doc comment: symmetric >>> / <<< wraps the whole
        // diagnostic, symmetric ~> / <~ wraps each supplementary block.
        assert!(out.starts_with(">>> error[TYPE-101]: type mismatch"));
        assert!(out.trim_end().ends_with("<<<"));
        let opens  = out.lines().filter(|l| l.trim_start().starts_with("~>")).count();
        let closes = out.lines().filter(|l| l.trim() == "<~").count();
        assert_eq!(opens, 2, "expected one ~> per secondary span plus one for help");
        assert_eq!(opens, closes, "every ~> must have exactly one matching <~");

        // Primary block is indented one level, secondary block one
        // level deeper — this is the exact bug a hand-picked-string
        // version of this function got wrong (see render_span_block's
        // doc comment).
        assert!(out.contains("  --> 3:12"));
        assert!(out.contains("    --> 1:8"));
    }

    #[test]
    fn underline_clamps_to_end_of_line_not_span_len() {
        // A 3-char-wide line with a span claiming length 50 must not
        // draw an underline 50 characters long.
        let source = "x=1\n";
        let diag = Diagnostic {
            code:          "LEX-001",
            message:       "test".to_string(),
            primary_span:  span(0, 50, 1, 1),
            primary_label: None,
            secondary:     Vec::new(),
            suggestion:    None,
        };
        let out = render(&diag, source);
        let underline_line = out.lines().find(|l| l.contains('^')).unwrap();
        assert_eq!(underline_line.matches('^').count(), 3);
    }
}
