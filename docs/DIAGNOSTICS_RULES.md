# Ubel Stratum — Diagnostics Rules

> **Canonical reference for every error, warning, and suggestion the
> compiler ever prints.
> Every contributor reads this before touching `error_management/`.**

---

## 0. Why this document exists

A language lives or dies by how well it tells you what you did wrong.
Rust's error messages are good not by accident — they name the exact
problem, point at the exact span, and usually tell you the exact fix.
That's the bar for Ubel Stratum's diagnostics, and until this document
existed, the compiler wasn't close to it: three separate, drifting
renderers existed (`diagnostics.rs::DiagnosticFormatter`,
`logger.rs::Logger::formatted_error`, and an inline copy in
`error_manager.rs`), only one was ever actually wired up, none of them
drew more than a single `^` regardless of span length, and the one tool
a person actually runs (`diagnose.rs`, and by extension the CI pipeline
dashboard) printed raw Rust `{:?}` Debug dumps instead of any of them.

All of that is fixed as of this document. There is now exactly one
renderer (`error_management/render.rs`), every error type implements
one trait (`Diagnosable`) to feed it, and `diagnose.rs` calls it. This
document is the spec for that renderer and the house style for every
message that flows through it — read it before adding a new error
variant, not after.

---

## 1. The format, worked example

Every diagnostic renders as this shape. This is real, current output —
not a mockup — captured from `err_duplicate_def.ubl`:

```
>>> error[NAME-002]: `x` is already defined in this scope
  --> 7:5
    |
  7 |     let x = 2    // ERROR: `x` is already defined in this scope
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  ~> note: first defined here
    --> 6:5
      |
    6 |     let x = 1
      |     ^^^^^^^^^
  <~
<<<
```

Line by line:

| Line | What it is |
|---|---|
| `>>> error[CODE]: message` | Always the first line. `CODE` is a stable, greppable identifier (§4). `message` is one sentence, lowercase after the colon, no trailing period (§2). `>>>` opens the whole-diagnostic fold region — see §5. |
| `  --> line:col` | The primary location. Two-space indent, always. |
| `    \| ` / `  N \| <source>` | The gutter and the exact source line the span is on. Gutter width tracks the line number's own digit count (`104 \|` is wider than `7 \|`) — this is the one place a hand-rolled fixed-width gutter breaks, so don't reintroduce one. |
| `    \| ^^^^^^^^` | The underline. **Full width of the span, not one character** — this is the whole reason `render.rs` exists instead of the three things it replaced. |
| `  ~> note: ...` | Opens a secondary-span block with its own nested `-->`/gutter/underline, indented one level deeper. Optional — only errors that reference another location use it (right now: `NameError::DuplicateDefinition`, `TypeError::TypeMismatch` when `because_of` is set). |
| `  <~` | Closes the `~>` block directly above it. Every `~>` has exactly one matching `<~`, even a one-line `help` with no nested span. |
| `  ~> help: ...` | Opens a plain-text suggestion block, no location. Optional. Also closed by its own `  <~`. |
| `<<<` | Closes the whole diagnostic. Pairs with the opening `>>>`. Always the last line. See §5. |

---

## 2. Message-writing rules

These apply to every `message()` and `suggestion()`/`~> help:` body in
`error_types/`.

1. **Lowercase after the colon**, matching rustc: `error: unexpected
   token`, not `error: Unexpected token`. Three of the four error type
   files were inconsistent about this before this document — check
   your new message against a sibling in the *same* file, not just
   against memory, since the inconsistency was file-by-file, not
   scattered.
   - Exception: a token, type name, or identifier in backticks keeps
     its own casing (`` `List<int>` ``), and so does an acronym used as
     a proper noun (`LINQ query expressions...` is correct as-is —
     the L isn't an artificially-capitalized sentence-starter, it's an
     acronym that's always uppercase).
   - Exception: tier names (`HIGH`, `MID`, `LOW`) stay uppercase
     wherever they appear, matching how `docs/MEMORY_MODEL.md` refers
     to them everywhere else. Treat them as proper nouns for this
     language, not as English words that happen to need emphasis.
2. **No trailing period** on the `message()` line. `~> help:` and
   `~> note:` text also skip the trailing period — matches the rest of
   the format's terse, label-like tone.
3. **State the fix, not just the problem**, whenever there IS a fix
   that isn't obvious from the message alone. `TypeMismatch` doesn't
   need a `suggestion()` — "expected `int`, found `string`" already
   tells you everything. `ArenaRefEscapesBoundary` does — "you can't do
   that" is true but useless without "restructure with the callback
   pattern instead." If you can't write a suggestion that adds real
   information beyond the message, leave `suggestion()` returning
   `None` for that variant rather than padding with something generic
   like "fix this error."
4. **Use backticks around anything the person would type or see in
   their own source**: types, identifiers, keywords, operators. Never
   backtick a category name (`type mismatch`, not `` `type mismatch` ``).
5. **Never use `{:?}` (Debug) to format a user-facing value that has a
   real `Display` impl.** `TokenType` has one — `parse_error.rs` used
   to print `` unexpected `Ident("foo")` `` via `{:?}` instead of the
   readable `` unexpected `foo` `` via `{}`. This was a real bug, now
   fixed, and the kind of thing to check for in any new message that
   embeds a token or type.
6. **Never use emoji.** Not in a message, not in a suggestion, not in
   a caller's `println!`/`eprintln!` around a diagnostic. This isn't
   stylistic pickiness: `diagnose.rs`'s output is captured to a file
   and re-displayed inside an HTML `<pre>` block by the pipeline
   dashboard, and an emoji is exactly as disruptive there as an ANSI
   escape code is (see §6) — it's noise a machine has to work around,
   not information. `crates/parser/src/main.rs` had `❌`/`✅` in its
   log lines before this document; they're gone now. If you're
   tempted to reach for one to mark pass/fail, `status: OK` /
   `status: FAIL` (already the house convention in `diagnose.rs`) says
   the same thing and greps cleanly.

---

## 3. Secondary spans

Use `secondary_spans()` (default: empty) whenever an error is fully
explained only by pointing at TWO places, not one — "you did X here,
but Y already happened there." The pattern, from `NameError`:

```rust
fn secondary_spans(&self) -> Vec<(Span, String)> {
    match self {
        NameError::DuplicateDefinition { first_defined, .. } =>
            vec![(*first_defined, "first defined here".to_string())],
        _ => Vec::new(),
    }
}
```

Don't reach for this to explain *why* a rule exists (that's what
`suggestion()`/`~> help:` is for) — only for a genuine second location.
`TypeError::TypeMismatch`'s `because_of` field is the other real user
of this today: when a type was established somewhere other than the
mismatch site, `~> note: expected type was established here` points at
it instead of the old text-only `"Type was established at line N"` —
show the actual line, don't just cite a line number.

---

## 4. Error Code Registry

Format: `<PHASE>-<NNN>`. Stable once assigned — never renumber an
existing code, even if the variant's wording changes. If a variant is
removed, retire its code; don't reassign the number to something else.

`TypeError` deliberately splits into two number ranges instead of one
flat `TYPE-0xx` — see §7, this is intentional groundwork for the
proposed folder split, not an accident of two people numbering things
differently.

### LEX-0xx — `error_types/lexical_error.rs`

| Code | Variant |
|---|---|
| LEX-001 | UnexpectedChar |
| LEX-002 | UnterminatedString |
| LEX-003 | UnterminatedBlockComment |
| LEX-004 | InvalidNumber |
| LEX-005 | InvalidEscape |
| LEX-006 | InvalidInterpolation |
| LEX-007 | InvalidCharLiteral |

### PARSE-0xx — `error_types/parse_error.rs`

| Code | Variant |
|---|---|
| PARSE-001 | UnexpectedToken |
| PARSE-002 | UnexpectedEof |
| PARSE-003 | UnclosedDelimiter |
| PARSE-004 | IllegalInContext |
| PARSE-005 | Raw |

### NAME-0xx — `error_types/name_error.rs`

| Code | Variant |
|---|---|
| NAME-001 | UndefinedName |
| NAME-002 | DuplicateDefinition |
| NAME-003 | UnresolvedImport |
| NAME-004 | UnresolvedPathSegment |
| NAME-005 | SelfOutsideMethod |
| NAME-006 | UnresolvedTypeParam |

### TYPE-1xx — ordinary type checking, `error_types/type_error.rs`

| Code | Variant |
|---|---|
| TYPE-101 | TypeMismatch |
| TYPE-102 | ArgumentCountMismatch |
| TYPE-103 | NoSuchField |
| TYPE-104 | NoSuchMethod |
| TYPE-105 | TryOnNonFallible |
| TYPE-106 | AwaitOnNonTask |
| TYPE-107 | CannotInferType |
| TYPE-108 | GenericArgCountMismatch |

### TYPE-2xx — tier & arena enforcement, `error_types/type_error.rs`

| Code | Variant |
|---|---|
| TYPE-201 | ArenaInWrongTier |
| TYPE-202 | AwaitInWrongTier |
| TYPE-203 | AsyncFunctionNotHigh |
| TYPE-204 | ArenaRefEscapesBoundary |
| TYPE-205 | IllegalTierCall |
| TYPE-206 | MidReturnContainsArenaRef |
| TYPE-207 | LinqInWrongTier |

**Adding a new variant:** append at the end of its range (don't
renumber to keep things "tidy" — see above), add the `code()` match
arm, and add a row to this table in the same change. A variant without
a code doesn't compile (`Diagnosable::code` has no default) — the
registry can't silently drift out of sync with the enum the way the
old inline suggestion text used to drift from what `report_section`
actually printed.

---

## 5. Fold markers

Every fold region has an explicit, symmetric open AND close marker —
nothing is inferred from an implicit "this line's shape means the
region started here." A dumb line-scanner — a regex, an editor
extension, `grep` — can then carve up a rendered diagnostic at *any*
nesting depth by matching open/close pairs alone, no
indentation-sniffing, no real parser needed. This matters today
because every consumer (the pipeline dashboard, this very document's
worked example) is working from captured plain text, and it matters
later because it's the same shape an LSP client will eventually want
to fold in an editor:

- **`>>>` / `<<<`** — wraps the whole diagnostic. `>>>` opens the
  `error[CODE]: message` line itself; `<<<` is the diagnostic's last
  line, alone, closing it.
- **`~>` / `<~`** — wraps each individual *supplementary* block: a
  `note` (points at a secondary span) or a `help` (plain suggestion,
  no span). Every `~>` has exactly one matching `<~`, even a one-line
  `help` with no nested span block of its own — no "sometimes
  symmetric" exception to remember or special-case in a parser.

The one thing deliberately **not** wrapped in its own marker pair is
the primary span block (`-->`/`^^^^`, right after the `>>> error[...]`
line) — that's the one fact a fold-aware reader should never be able
to collapse away. Only the `~>`/`<~` blocks are meant to fold
independently of it and of each other.

No warning severity exists yet (every current error type is, well, an
error), so `warning[...]` is aspirational — but the marker scheme
already accounts for it: `>>> warning[...]` / `<<<` works identically,
nothing about `~>`/`<~` needs to change when warnings are added.

When real LSP support lands, `Diagnostic` (the struct `render()`
consumes) maps close to 1:1 onto LSP's own `Diagnostic` type — `code`,
`message`, `range` from `primary_span`, `relatedInformation` from
`secondary`. These text markers are the interim, file-based version of
the same split, not a dead end that gets thrown away once an LSP
exists.

---

## 6. Plain text by design — no ANSI, no color, ever, in `render()`

`render()` never emits an escape code. Two reasons, both load-bearing:

1. `diagnose.rs`'s output is always captured to a file
   (`results/<fixture>.txt`) and re-displayed inside an HTML `<pre>`
   block by `scripts/build_dashboard_report.py` — the same dashboard
   the pipeline results screenshot in this project's history came
   from. An embedded escape code shows up there as literal `\x1b[31m`
   garbage, not color.
2. One renderer, one behavior, everywhere it's called from beats a
   renderer with a color flag that's right in one caller and wrong in
   another. `error_manager.rs::report_all` — the legacy `crates/parser`
   CLI's error path, the one place a human might plausibly be looking
   at a real terminal — prints this exact same plain output for that
   reason, not because color wouldn't be nice there too.

If a genuinely interactive terminal consumer shows up later (a REPL),
it colorizes by wrapping `render()`'s plain output line-by-line, not by
teaching `render()` a second mode. Keep the one renderer honest.

---

## 7. Known limitation: multi-line spans

`Span` (`crates/core/src/lexer/token.rs`) tracks `start`/`end` as byte
offsets plus a single `line`/`column` pair for the **start** position
only — there is no `end_line`/`end_column`. For the overwhelming
majority of spans (an identifier, an operator, most expressions) this
is irrelevant since they don't cross a line boundary. For a span that
genuinely does — or, as happened in the worked example in §1, a span
that's simply wider than intended and runs past the end of its line —
`render_span_block` clamps the underline to however many characters
are actually left on that one source line, rather than either
panicking on an out-of-bounds slice or drawing a nonsensically long
underline that wraps visual garbage onto the next line.

You can see this clamping in the §1 example directly: `x`'s
`DuplicateDefinition` span runs a little past the end of `let x = 2`
(onto the trailing comment / towards the next statement), and the
underline visibly stops at the end of line 7 instead of continuing
past it. That's the clamp working as intended, not a rendering bug —
but it's also a sign the span itself is wider than it needs to be.
Tightening spans to their true minimal extent is a separate, lower-
priority cleanup from anything in this document; this section exists
so nobody re-discovers the clamping from scratch and "fixes" it into a
crash.

**If `Span` ever grows `end_line`/`end_column`:** `render_span_block`
is the one function that needs to learn to print multiple source lines
instead of clamping to one. Nothing else in `render.rs` assumes
single-line spans.

---

## 8. A bug this document's own worked examples caught

While building the renderer described in this document and actually
looking at real rendered output instead of trusting the underlying
data, a real, pre-existing bug turned up: `logos_lexer.rs` used
`#[logos(skip r"[ \t]+")]` to discard whitespace between tokens. A
`skip` match in `logos` is consumed with **no callback at all** — so
`update_position` (the function that advances `self.column`) never ran
for it. Every token's reported column silently undercounted by however
much whitespace had been skipped since the last real token, compounding
across a line. A token 8 spaces into an indented line was reported at
column 1, not column 9; a token in `    let mut outer` was reported at
column 7, not 13.

This meant that before this fix, **every column in every diagnostic the
compiler had ever produced was potentially wrong** — the "arrow points
at the exact wrong place" failure mode this whole document exists to
prevent, hiding underneath a rendering layer that would have drawn a
perfectly formatted, perfectly confident underline in the wrong spot.

Fixed by making whitespace an explicit, position-tracked token
(`#[regex(r"[ \t]+")] Whitespace`) discarded in `handle_logos_token`
right alongside `Newline`/`LineComment` — same discard behavior, but
`update_position` runs first. If you ever add another `#[logos(skip
...)]` directive to this lexer, ask whether it can *ever* match
something with nonzero width before adding it — if it can, it needs
this same treatment, not `skip`.

---

## 9. Proposed: physical folder reorganization (not yet implemented)

Everything above this section is implemented and in the repo today.
This section is a proposal, written down so it can be reviewed and
revised before anyone spends an afternoon executing it — not a plan
already underway.

### What exists today

```
error_management/
├── error_manager.rs      (ErrorManager — accumulator, delegates to render.rs)
├── logger.rs             (Logger — plain leveled logging, unrelated to diagnostics)
├── render.rs             (THE renderer — Diagnostic, Diagnosable, render())
└── error_types/
    ├── lexical_error.rs  (LEX-0xx)
    ├── parse_error.rs    (PARSE-0xx)
    ├── name_error.rs     (NAME-0xx)
    └── type_error.rs     (TYPE-1xx ordinary + TYPE-2xx tier/arena, one enum)
```

This is already organized by *phase*, which is why the Error Code
Registry (§4) numbers by phase too. What it's not yet organized by is
*class within a phase* — "errors about dynamic naming" and "errors
about imports" are both just cases inside one flat `NameError` enum,
for instance.

### The proposed shape

```
error_management/
└── errors/
    ├── lexical/
    │   ├── mod.rs         (enum LexicalError — unchanged variants/codes)
    │   └── numbers.rs     (reference doc: every LEX-004 sub-case + example + fix)
    ├── parse/
    │   └── mod.rs
    ├── naming/                              <- renamed from name_error.rs
    │   ├── mod.rs          (enum NameError — unchanged variants/codes)
    │   ├── undefined.rs    (reference doc: NAME-001)
    │   └── duplicate.rs    (reference doc: NAME-002)
    ├── types/
    │   └── mod.rs          (TYPE-1xx variants only)
    └── tier/                                <- physically split out of TypeError
        └── mod.rs          (TYPE-2xx variants, renumbered TIER-xxx — see below)
```

Key decisions this proposal makes, for review rather than silent
execution:

1. **Enum variants and their codes stay put during the move.** Moving
   `NameError` from `error_types/name_error.rs` to `errors/naming/mod.rs`
   is a `mod` path change, not a rewrite — every existing call site
   (`name_resolution.rs`, etc.) keeps working unchanged as long as the
   type name and its public API don't change, which they wouldn't.
2. **The one real reshaping is TypeError → TypeError + TierError.**
   This is the one place where physically splitting also means
   splitting the *enum*, because TYPE-2xx already reads as its own
   family (§4) — the split is renumbering `TYPE-201..207` to
   `TIER-001..007`, keeping the *tail* stable per §4's own promise,
   and touching every call site that currently writes
   `TypeError::ArenaRefEscapesBoundary` (`type_infer.rs`, `tier_check.rs`)
   to write `TierError::RefEscapesBoundary` instead. This is the one
   part of the reorg with real blast radius — everywhere else is a
   file move.
3. **Per-class reference docs (`numbers.rs`, `undefined.rs`, etc.) are
   new content, not moved content** — one markdown-in-Rust-doc-comment
   page per error class, each with a "here's code that triggers this"
   snippet and "here's the fix" walkthrough, in the spirit of rustc's
   `--explain E0382`. Writing all ~33 of these is a separate, large
   effort that should happen *after* the structure above is confirmed,
   not before — no point writing NAME-001's reference page against a
   folder layout that might still change.

This section intentionally stops at "proposal." Confirm the shape
above (or amend it) before anyone starts moving files.
