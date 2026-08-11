#!/usr/bin/env python3
"""
build_job_summary.py

Renders the SAME report.json build_dashboard_report.py already produces
as GitHub-flavored markdown for the workflow run's own Job Summary tab
(the file at $GITHUB_STEP_SUMMARY) — a view of pipeline results that
doesn't depend on GitHub Pages being up at all. Added because Pages has
been unreliable (a real, documented Aug 6 2026 GitHub-wide incident
dropped this repo's own deploy — see git history around that date); this
is a fallback, not a replacement — web/dashboard's own richer HTML view
(source/token/AST detail per fixture, the Nord-derived syntax
highlighting) still only exists there. This script deliberately reuses
report.json rather than re-parsing results/*.txt independently, so the
two views can never silently disagree about what "sema: FAIL" means for
a given fixture.

Usage:
    python3 scripts/build_job_summary.py --report web/dashboard/report.json

Prints markdown to stdout — the workflow step appends it to
$GITHUB_STEP_SUMMARY itself (`>> "$GITHUB_STEP_SUMMARY"`), matching
GitHub's own recommended pattern of leaving append-vs-overwrite to the
caller rather than a script assuming it's the only step in the job that
ever writes there.
"""
import argparse
import json
import sys
from pathlib import Path

# Defensive cap, not a real expected case: this repo has ~50 fixtures
# today, nowhere near this limit. Exists so a future mass-failure (e.g.
# a real compiler regression breaking most of the suite at once) can't
# blow past GitHub's own step-summary size limit and silently truncate
# the *whole* summary instead of just the least-important part of it.
MAX_LISTED_FAILURES = 60


def fmt_ns(ns: float) -> str:
    """Human-scaled time, matching the dashboard HTML's own convention."""
    if ns >= 1_000_000_000:
        return f"{ns / 1_000_000_000:.2f} s"
    if ns >= 1_000_000:
        return f"{ns / 1_000_000:.2f} ms"
    if ns >= 1_000:
        return f"{ns / 1_000:.2f} us"
    return f"{ns:.0f} ns"


def stage_line(counts: dict) -> str:
    ok = counts.get("ok", 0)
    fail = counts.get("fail", 0)
    skipped = counts.get("skipped", 0)
    parts = [f"{ok} ok"]
    if fail:
        parts.append(f"{fail} fail")
    if skipped:
        parts.append(f"{skipped} skipped")
    return " · ".join(parts)


def failing_fixtures(report: dict) -> list[tuple[str, list[str]]]:
    """(fixture_name, [failed_stage, ...]) for every fixture with at
    least one non-ok, non-skipped stage. `skipped` isn't a failure —
    it's the expected downstream consequence of an earlier stage already
    failing (e.g. every err_*.ubl fixture's interpret stage is correctly
    'skipped', not a problem to flag)."""
    out = []
    for name, data in sorted(report.get("fixtures", {}).items()):
        summary = data.get("summary", {})
        failed_stages = [
            stage for stage in ("lex", "parse", "sema", "interpret")
            if "fail" in summary.get(stage, "").lower()
        ]
        if failed_stages:
            out.append((name, failed_stages))
    return out


def build_markdown(report: dict) -> str:
    sha = report.get("commit_sha", "")[:12]
    run_url = report.get("run_url", "")
    generated_at = report.get("generated_at", "")
    counts = report.get("stage_counts", {})
    fixtures = report.get("fixtures", {})
    benchmarks = report.get("benchmarks", [])
    failures = failing_fixtures(report)

    lines = []
    lines.append("## Pipeline Dashboard")
    lines.append("")
    lines.append(
        f"`{sha}` · {len(fixtures)} fixtures · generated {generated_at}"
        + (f" · [full run]({run_url})" if run_url else "")
    )
    lines.append("")
    lines.append(
        "> GitHub Pages can lag or drop a deploy (a real incident did "
        "exactly this on 2026-08-06) — this summary is built from the "
        "same report.json as the Pages dashboard, so it's never stale "
        "relative to Pages; if anything, Pages can be stale relative to "
        "*this*, not the other way around."
    )
    lines.append("")

    lines.append("### Stages")
    lines.append("")
    lines.append("| Stage | Result |")
    lines.append("|---|---|")
    for stage in ("lex", "parse", "sema", "interpret"):
        lines.append(f"| {stage} | {stage_line(counts.get(stage, {}))} |")
    lines.append("")

    if failures:
        lines.append(f"### Failing fixtures ({len(failures)})")
        lines.append("")
        lines.append("| Fixture | Failed at |")
        lines.append("|---|---|")
        for name, stages in failures[:MAX_LISTED_FAILURES]:
            lines.append(f"| `{name}` | {', '.join(stages)} |")
        if len(failures) > MAX_LISTED_FAILURES:
            lines.append(f"| ... | +{len(failures) - MAX_LISTED_FAILURES} more |")
        lines.append("")
        lines.append(
            "Fixtures named `err_*` are *expected* to fail sema/parse — "
            "check the fixture's own name and header comment before "
            "treating a row here as a regression."
        )
        lines.append("")
    else:
        lines.append("### Failing fixtures")
        lines.append("")
        lines.append("None — every fixture landed exactly where its name says it should.")
        lines.append("")

    if benchmarks:
        lines.append(f"### Benchmarks ({len(benchmarks)})")
        lines.append("")
        lines.append("| Benchmark | Mean | Std dev |")
        lines.append("|---|---|---|")
        for b in benchmarks:
            mean = fmt_ns(b["mean_ns"])
            std = fmt_ns(b["std_dev_ns"]) if b.get("std_dev_ns") is not None else "-"
            lines.append(f"| `{b['name']}` | {mean} | {std} |")
        lines.append("")

    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", required=True, type=Path)
    args = ap.parse_args()

    if not args.report.exists():
        print(f"error: report not found: {args.report}", file=sys.stderr)
        sys.exit(1)

    report = json.loads(args.report.read_text())
    print(build_markdown(report))


if __name__ == "__main__":
    main()
