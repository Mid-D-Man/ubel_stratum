# Ubel Stratum — Print & Format-Spec Rules

> Canonical reference for `print`/`println`/`log`, string interpolation,
> and the `{expr:spec}` format-spec syntax inside interpolation holes.
> First slice landed this session — see §4 for what's deliberately
> deferred, and §5 for what this delivery fixed along the way that
> wasn't originally in scope.

---

## 1. What exists

`print`/`println`/`log` are variadic global builtins
(`interpreter/builtins.rs`) that join their arguments with spaces and
write through `Value: Display`. String interpolation
(`$"...{expr}..."`, plus its verbatim form `$@"...{expr}..."`) embeds
expressions directly in a string literal — parsed once, at the same
time as everything else (`rd_parser::parsers::parse_expr::parse_interp`),
not re-parsed later by sema or the interpreter.

`Value: Display` already renders compound types close to Rust's `{:?}`
derive shape — `TypeName { field: value, ... }` for structs, recursing
correctly through nested collections — not just bare primitives.

## 2. Format-spec syntax

```
{expr}                — unchanged, no spec
{expr:spec}            — spec = [align][width][.precision][?]
align      ::= "<" | ">" | "^"        (left / right / center)
width      ::= digit+
precision  ::= digit+
debug      ::= "?"
```

Examples: `{n:>10}` (right-align, width 10), `{pi:.2}` (2 decimal
places), `{pi:>10.2}` (both together), `{name:^20}` (centered), `{x:?}`
(reserved debug flag — see §4).

### How the spec is found — parser-level, not lexer-level

A hole's raw token stream is fully tokenized once
(`string_parser.rs::parse_interpolation_expr`), then `parse_expr` parses
one real expression from it. Whatever the parser doesn't recognize as
part of that expression is leftover; if the leftover starts with `:`,
everything after it is parsed as a format spec
(`parse_expr.rs::parse_format_spec`) — a small, separate grammar, not
Ubel Stratum expression syntax. Any other leftover content is a real
parse error (`PARSE-004`) — previously (see §5) it was silently
discarded instead.

This has to happen at the parser level because only the parser actually
knows where an expression ends; the lexer has no notion of expression
structure. **Ubel Stratum has no ternary operator** — `?` is the
fallible-unwrap postfix (same as Rust's `?`), not a conditional — so
there's no `cond ? a : b`-style `:` for a format-spec `:` to collide
with. The leftover-token approach is still the right general mechanism
(it correctly separates "the expression" from "everything else" for any
reason, not just one specific collision), it just doesn't have that
particular ambiguity to resolve in this language.

### The `width.precision` token collision

`{value:10.2}` (width and precision back-to-back, no separator) is
genuinely ambiguous at the *token* level: the general tokenizer has no
notion of format-spec context, so a digit immediately before a `.`
lexes as one `Float`/`DoubleLit` token, the same as it would anywhere
else in the language (confirmed: `{value:.2}` alone is fine, since a
leading `.` with no digit before it never forms a numeric literal).
`parse_format_spec` recovers width/precision from the literal's own
source *text*, not its parsed `f32`/`f64` value — a float's fractional
part doesn't reliably round-trip back to "how many digits were
written" (`10.20` and `10.2` are the same `f64`, different precisions).

## 3. Type checking

`.precision` only applies to `Float`/`Double` (decimal places) or `Str`
(max length, truncating) — anything else is `TypeError::
InvalidFormatSpec` (`TYPE-115`, `DIAGNOSTICS_RULES.md`). Width/align/`?`
apply to any type, since padding/truncation operate on the rendered
string regardless of what produced it.

## 4. Deliberately deferred (not in this slice)

- **Custom fill character.** Padding is always spaces. Rust's
  `[[fill]align]` (e.g. `{value:*>10}`) isn't supported — only bare
  align markers.
- **Sign forcing (`+`).** No way to force a `+` on positive numbers.
- **Alternate form (`#`) / zero-padding (`0`).** Neither exists.
- **Numeric bases** (`x`/`X`/`o`/`b` for hex/octal/binary). Not
  supported — everything renders in the type's normal `Display` base.
- **A real `Debug`-vs-`Display` split.** The `?` flag parses and
  type-checks but is currently a no-op at the value level — there's
  only one formatter (`Value: Display`), so `{x:?}` and `{x}` render
  identically today. The syntax is wired through end-to-end
  specifically so this means something the moment a real split exists,
  rather than needing a second syntax-plus-parser change later.
- **A `format!`/positional-args function** separate from interpolation.
  Everything goes through `$"..."` holes; there's no `format(spec,
  arg1, arg2, ...)`-style call.

## 5. What this delivery fixed along the way (not originally in scope)

Two real, pre-existing gaps surfaced while landing format specs — both
fixed here, not deferred, since each was directly blocking verification
of the feature itself:

**Interpolation holes were never type-checked.** `infer_literal`'s
`InterpolatedStr`/`InterpolatedVerbatimStr` arm returned `Str`
immediately without ever calling `infer_expr` on any hole's expression.
A hole referencing an undefined name, or containing any other real type
error, sailed through sema completely undetected — `$"{undefined_var}"`
was accepted. Fixed as a direct consequence of needing each hole's type
anyway, to validate its format spec against it
(`err_interpolation_hole_undefined_name.ubl`).

**Name resolution never visited interpolation holes either — one layer
earlier, and the actual root cause of the above.** `resolve_expr`'s
`ExprKind::Lit(_) => {}` arm matched `InterpolatedStr` too, so nothing
inside a `{expr}` hole was ever registered in the `resolutions` map.
Invisible at runtime — the interpreter resolves identifiers by name
(`interp.lookup`, `eval/expr.rs`), completely independent of sema's
resolution map — which is exactly why nobody had noticed: interpolated
variable references always *worked*, they just were never *checked*.
Surfaced the moment `infer_literal` started calling `infer_expr` on
hole expressions and got `<unknown>` back for a perfectly ordinary
bound variable.

**A second interpolated string anywhere in a file broke the lexer —
a real, previously-documented, known gap** (see
`ok_collections_full.ubl`'s original header comment: *"Only ONE
interpolated string ... a second one anywhere later currently breaks
the lexer (known gap)"*). Root cause: `LogosLexer::handle_logos_token`
rebases its underlying `logos::Lexer` onto a subslice after every
interpolated/verbatim string and every block/doc comment
(`self.logos_lex = LogosToken::lexer(&self.input[pos..])`), which makes
`self.logos_lex.span()` relative to that rebase point, not to the
original source. The code was using that rebase-relative `span_range`
directly as if it were an absolute file offset — both for ordinary
token spans and, critically, as the starting position handed to the
*next* string/comment sub-parser. `self.position`, by contrast, is a
plain running byte counter (`update_position`) that's never reset by a
rebase, so it's the one value that was always trustworthy. Fixed by
computing every absolute position from `self.position` instead of
`span_range` directly. This was a real, general lexer bug — not
specific to format specs, and not specific to interpolation; it would
have affected multiple block/doc comments after a string too. Confirmed
fixed via `ok_multi_interpolation_and_format_spec.ubl` (several
interpolated strings, several holes each) and the full existing fixture
suite re-passing unchanged.

## 6. Open questions for the next slice

- Fill character, sign, `#`, `0`-padding, and numeric bases — real
  Rust-parity would want all of these; §4 has the exact list.
- Whether a real `Debug`-vs-`Display` split is worth building before
  the `?` flag has anything to differentiate, or whether it should wait
  for a concrete need (e.g. a `derive`-style mechanism, or a
  user-overridable formatting hook on structs).
- Whether nested string literals inside a `{}` hole should be
  supported — currently `$"{cond} {"literal"}"`-style holes containing
  their own `"..."` string will confuse the *outer* interpolated
  string's own boundary scan (a separate, real, still-open limitation,
  not touched by this delivery — the outer scanner doesn't track
  whether it's "inside a hole" when looking for the string's own
  closing `"`).
