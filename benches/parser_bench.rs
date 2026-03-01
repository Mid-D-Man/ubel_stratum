// benches/parser_bench.rs
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

use ubel_stratum::ast::arena::AstArena;
use ubel_stratum::{lexer, parser};

// ── Source fixtures ───────────────────────────────────────────────

const EMPTY_PROGRAM: &str = "";

const HELLO_WORLD: &str = r#"
package demo

fn main() {
    let msg = "Hello, World!"
    return
}
"#;

const SIMPLE_STRUCT: &str = r#"
struct Point {
    x: int
    y: int

    pub fn new(x: int, y: int) Point {
        return Point { x = x, y = y }
    }

    pub fn distance(self) float {
        return 0.0
    }
}
"#;

const ENUM_AND_MATCH: &str = r#"
enum Status {
    Active,
    Inactive,
    Pending = 3,
}

fn describe(s: Status) string {
    match s {
        Status.Active   => "active",
        Status.Inactive => "inactive",
        _               => "unknown",
    }
}
"#;

const ASYNC_FUNCTION: &str = r#"
package server

async fn handle(req: string) Task<string>! {
    let result = await fetch(req)?
    return result
}
"#;

const GENERICS_AND_TRAITS: &str = r#"
trait Printable {
    fn print(self)
}

struct Container<T> {
    value: T

    pub fn get(self) T {
        return self.value
    }
}

impl Printable for Container<int> {
    pub fn print(self) {
        return
    }
}
"#;

const CONTROL_FLOW: &str = r#"
fn fizzbuzz(n: int) string {
    let mut i = 0
    let mut result = ""
    while i < n {
        if i % 15 == 0 {
            result = "FizzBuzz"
        } elif i % 3 == 0 {
            result = "Fizz"
        } elif i % 5 == 0 {
            result = "Buzz"
        } else {
            result = "number"
        }
        i += 1
    }
    return result
}
"#;

const ARENA_AND_WITH: &str = r#"
@tier(mid)
fn parse_data(input: string) string {
    with arena(1MB) {
        let tokens = tokenize(input)
        return process(tokens)
    }
}
"#;

const PATTERN_MATCHING: &str = r#"
fn process(val: int) string {
    match val {
        0       => "zero",
        1..10   => "small",
        10..=99 => "medium",
        _       => "large",
    }
}
"#;

const LARGE_PROGRAM: &str = r#"
package large_program

summon std.io
summon std.collections.List
from std.collections summon [Dictionary, Set]

const MAX_SIZE: int = 1000
const VERSION: string = "1.0.0"

type Result<T> = T!

struct Config {
    host: string
    port: int
    debug: bool

    pub fn new(host: string, port: int) Config {
        return Config { host = host, port = port, debug = false }
    }

    pub fn with_debug(mut self, debug: bool) Config {
        self.debug = debug
        return self
    }
}

enum LogLevel {
    Debug = 0,
    Info  = 1,
    Warn  = 2,
    Error = 3,
}

trait Logger {
    fn log(self, level: LogLevel, msg: string)
    fn debug(self, msg: string) {
        self.log(LogLevel.Debug, msg)
    }
}

struct ConsoleLogger {
    prefix: string

    pub fn new(prefix: string) ConsoleLogger {
        return ConsoleLogger { prefix = prefix }
    }
}

impl Logger for ConsoleLogger {
    pub fn log(self, level: LogLevel, msg: string) {
        return
    }
}

extend string {
    fn shorten(self, max: int) string {
        return self
    }
}

fn parse_config(input: string) Config! {
    with arena(512MB) {
        let raw = parse_raw(input)?
        return Config.new(raw.host, raw.port)
    }
}

async fn start_server(cfg: Config) Task<void>! {
    let logger = ConsoleLogger.new("server")
    logger.debug("Starting...")
    let mut i = 0
    while i < MAX_SIZE {
        let item = fetch_item(i)?
        match item {
            Ok(data) where data != "" => {
                process(data)
            }
            Ok(_) => {}
            Err(e) => {
                fail e
            }
        }
        i += 1
    }
}

fn compute(values: List<int>) int {
    let mut total = 0
    for v in values {
        total += v
    }
    return total
}

fn classify(n: int) string {
    match n {
        0          => "zero",
        1..10      => "small",
        10..=99    => "medium",
        100..=999  => "large",
        _          => "huge",
    }
}
"#;

// ── Helpers ────────────────────────────────────────────────────────

fn lex_and_parse(source: &str) {
    let tokens = lexer::tokenize(source).expect("lex failed");
    let arena = AstArena::with_capacity(256 * 1024);
    let _ = parser::parse(&arena, tokens, source.to_string());
}

fn lex_only(source: &str) -> Vec<ubel_stratum::Token> {
    lexer::tokenize(source).expect("lex failed")
}

// ── 1. Lex-only baseline (so we can isolate parser cost) ──────────

fn bench_lex_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/lex_baseline");
    group.measurement_time(Duration::from_secs(4));

    let cases: &[(&str, &str)] = &[
        ("hello_world",   HELLO_WORLD),
        ("control_flow",  CONTROL_FLOW),
        ("large_program", LARGE_PROGRAM),
    ];

    for (name, src) in cases {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), src, |b, s| {
            b.iter(|| black_box(lex_only(black_box(s))));
        });
    }

    group.finish();
}

// ── 2. Full parse: small programs ─────────────────────────────────

fn bench_parse_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/small");
    group.measurement_time(Duration::from_secs(4));

    let cases: &[(&str, &str)] = &[
        ("empty",          EMPTY_PROGRAM),
        ("hello_world",    HELLO_WORLD),
        ("simple_struct",  SIMPLE_STRUCT),
        ("enum_and_match", ENUM_AND_MATCH),
        ("async_fn",       ASYNC_FUNCTION),
        ("pattern_match",  PATTERN_MATCHING),
        ("arena_with",     ARENA_AND_WITH),
    ];

    for (name, src) in cases {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), src, |b, s| {
            b.iter(|| lex_and_parse(black_box(s)));
        });
    }

    group.finish();
}

// ── 3. Full parse: medium programs ────────────────────────────────

fn bench_parse_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/medium");
    group.measurement_time(Duration::from_secs(5));

    let cases: &[(&str, &str)] = &[
        ("generics_and_traits", GENERICS_AND_TRAITS),
        ("large_program",       LARGE_PROGRAM),
    ];

    for (name, src) in cases {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), src, |b, s| {
            b.iter(|| lex_and_parse(black_box(s)));
        });
    }

    group.finish();
}

// ── 4. Arena allocation cost vs parse cost ────────────────────────

fn bench_arena_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/arena_sizes");
    group.measurement_time(Duration::from_secs(4));

    let tokens = lex_only(LARGE_PROGRAM);

    for capacity_kb in [32usize, 64, 128, 256, 512] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", capacity_kb)),
            &capacity_kb,
            |b, &cap| {
                b.iter(|| {
                    let arena = AstArena::with_capacity(cap * 1024);
                    let _ = parser::parse(
                        &arena,
                        tokens.clone(),
                        LARGE_PROGRAM.to_string(),
                    );
                    black_box(arena.allocated_bytes());
                });
            },
        );
    }

    group.finish();
}

// ── 5. Repeated parse of the same source (hot-path / cache) ───────

fn bench_repeated_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/repeated");
    group.measurement_time(Duration::from_secs(5));

    // Pre-lex once to isolate pure parse time
    let tokens = lex_only(LARGE_PROGRAM);

    group.throughput(Throughput::Bytes(LARGE_PROGRAM.len() as u64));
    group.bench_function("large_program_prelex", |b| {
        b.iter(|| {
            let arena = AstArena::with_capacity(256 * 1024);
            let _ = parser::parse(
                black_box(&arena),
                tokens.clone(),
                LARGE_PROGRAM.to_string(),
            );
        });
    });

    group.finish();
}

// ── 6. Synthetic scale: N copies of a function ────────────────────

fn gen_n_functions(n: usize) -> String {
    let mut src = String::from("package scale\n\n");
    for i in 0..n {
        src.push_str(&format!(
            "fn func_{i}(x: int, y: int) int {{\n    return x + y\n}}\n\n"
        ));
    }
    src
}

fn bench_scale_functions(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/scale_functions");
    group.measurement_time(Duration::from_secs(5));

    for count in [10usize, 50, 100, 200] {
        let src = gen_n_functions(count);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            &src,
            |b, s| {
                b.iter(|| lex_and_parse(black_box(s)));
            },
        );
    }

    group.finish();
}

// ── 7. Expression depth stress ────────────────────────────────────

fn gen_deep_expr(depth: usize) -> String {
    let mut expr = "1".to_string();
    for i in 2..=depth {
        expr = format!("{} + {}", expr, i);
    }
    format!("fn deep_expr() int {{\n    return {}\n}}\n", expr)
}

fn bench_deep_expressions(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser/deep_expr");
    group.measurement_time(Duration::from_secs(4));

    for depth in [16usize, 64, 128, 256] {
        let src = gen_deep_expr(depth);
        group.throughput(Throughput::Elements(depth as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("depth_{}", depth)),
            &src,
            |b, s| {
                b.iter(|| lex_and_parse(black_box(s)));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_lex_baseline,
    bench_parse_small,
    bench_parse_medium,
    bench_arena_sizes,
    bench_repeated_parse,
    bench_scale_functions,
    bench_deep_expressions,
);
criterion_main!(benches);
