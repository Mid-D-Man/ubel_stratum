#!/usr/bin/env python3
"""
build_dashboard_report.py

Turns raw output from two sources into one report.json for the dashboard:

  1. Per-fixture diagnose output (crates/rd_parser/examples/diagnose.rs),
     one .txt file per .ubl fixture, section-delimited with "=== NAME ===".
  2. Criterion's own estimates.json files under target/criterion/**, one
     per benchmark.

Deliberately separate from both the Rust tool that produces (1) and the
HTML page that renders the result — this script only transforms data, it
doesn't generate or display it.

Usage:
    python3 scripts/build_dashboard_report.py \
        --results-dir results/ \
        --criterion-dir target/criterion \
        --out web/dashboard/report.json \
        --commit-sha <sha> --run-url <url>
"""
import argparse
import json
import re
import sys
from pathlib import Path

SECTION_RE = re.compile(r"^=== (.+?) ===\s*$")
SUMMARY_LINE_RE = re.compile(r"^(\w+):\s+(.+)$")


def parse_diagnose_output(text: str) -> dict:
    """Split one diagnose.rs report into its named sections."""
    sections: dict[str, list[str]] = {}
    current = "PREAMBLE"
    sections[current] = []
    for line in text.splitlines():
        m = SECTION_RE.match(line)
        if m:
            current = m.group(1).strip()
            # "FILE: <path>" is its own section header with content baked in
            if current.startswith("FILE:"):
                sections["FILE"] = [current[len("FILE:"):].strip()]
                current = "PREAMBLE"
            sections.setdefault(current, [])
            continue
        sections[current].append(line)

    summary = {}
    for line in sections.get("SUMMARY", []):
        m = SUMMARY_LINE_RE.match(line.strip())
        if m:
            summary[m.group(1)] = m.group(2).strip()

    return {
        "source": "\n".join(sections.get("SOURCE", [])).strip("\n"),
        "tokens": "\n".join(sections.get("TOKENS", [])).strip("\n"),
        "parse": "\n".join(sections.get("PARSE", [])).strip("\n"),
        "sema": "\n".join(sections.get("SEMA", [])).strip("\n"),
        "interpret": "\n".join(sections.get("INTERPRET", [])).strip("\n"),
        "summary": summary,
    }


def collect_fixture_reports(results_dir: Path) -> dict:
    fixtures = {}
    for txt_path in sorted(results_dir.glob("*.txt")):
        name = txt_path.stem
        try:
            fixtures[name] = parse_diagnose_output(txt_path.read_text())
        except Exception as e:  # noqa: BLE001 — report, don't crash the build
            fixtures[name] = {
                "source": "", "tokens": "", "parse": "", "sema": "", "interpret": "",
                "summary": {"error": f"failed to parse diagnose output: {e}"},
            }
    return fixtures


def collect_benchmarks(criterion_dir: Path) -> list[dict]:
    """
    Criterion writes target/criterion/<group>/<id>/new/estimates.json (or
    base/estimates.json on a run with no comparison baseline). Walk the
    whole tree rather than assuming one exact layout, since that's varied
    slightly across criterion versions.
    """
    benches = []
    if not criterion_dir.exists():
        return benches

    for estimates_path in criterion_dir.rglob("estimates.json"):
        # Only take the "new" (latest run) copy; skip "base" (comparison
        # baseline) and "change" (diff) if a "new" one exists alongside it.
        if estimates_path.parent.name not in ("new", "base"):
            continue
        if estimates_path.parent.name == "base":
            sibling = estimates_path.parent.parent / "new" / "estimates.json"
            if sibling.exists():
                continue  # prefer the "new" copy, skip this "base" one

        try:
            data = json.loads(estimates_path.read_text())
        except Exception:
            continue

        # Path relative to target/criterion, minus the trailing
        # new|base/estimates.json, minus the report/ dir if present.
        rel_parts = estimates_path.relative_to(criterion_dir).parts[:-2]
        if not rel_parts:
            continue
        bench_name = "/".join(rel_parts)

        mean = data.get("mean", {}).get("point_estimate")
        std_dev = data.get("std_dev", {}).get("point_estimate")
        if mean is None:
            continue

        benches.append({
            "name": bench_name,
            "mean_ns": mean,
            "std_dev_ns": std_dev,
        })

    benches.sort(key=lambda b: b["name"])
    return benches


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--results-dir", required=True, type=Path)
    ap.add_argument("--criterion-dir", type=Path, default=None)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--commit-sha", default="")
    ap.add_argument("--run-url", default="")
    ap.add_argument("--generated-at", default="")
    args = ap.parse_args()

    if not args.results_dir.exists():
        print(f"error: results dir not found: {args.results_dir}", file=sys.stderr)
        sys.exit(1)

    fixtures = collect_fixture_reports(args.results_dir)
    benchmarks = collect_benchmarks(args.criterion_dir) if args.criterion_dir else []

    stage_counts = {"lex": {"ok": 0, "fail": 0}, "parse": {"ok": 0, "fail": 0, "skipped": 0},
                    "sema": {"ok": 0, "fail": 0, "skipped": 0},
                    "interpret": {"ok": 0, "fail": 0, "skipped": 0}}
    for f in fixtures.values():
        for stage in ("lex", "parse", "sema", "interpret"):
            status = f["summary"].get(stage, "skipped").lower()
            bucket = "fail" if "fail" in status else ("skipped" if status == "skipped" else "ok")
            stage_counts[stage][bucket] = stage_counts[stage].get(bucket, 0) + 1

    report = {
        "commit_sha": args.commit_sha,
        "run_url": args.run_url,
        "generated_at": args.generated_at,
        "stage_counts": stage_counts,
        "fixtures": fixtures,
        "benchmarks": benchmarks,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2))
    print(f"wrote {args.out} ({len(fixtures)} fixtures, {len(benchmarks)} benchmarks)")


if __name__ == "__main__":
    main()
