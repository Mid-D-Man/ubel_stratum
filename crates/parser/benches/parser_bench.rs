// ubel_stratum_parser/benches/parser_bench.rs
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

use ubel_stratum::ast::arena::AstArena;
use ubel_stratum::lexer;
use ubel_stratum_parser as parser;

// ── Source fixtures (valid Ubel Stratum syntax) ───────────────────────────────
// Syntax rules confirmed from grammar.lalrpop:
//   fn name(param: type) ReturnType { }   -- no '->', no colon before return
//   struct Name { field: type }            -- fields separated by newlines
//   Name { field = value, field2 = value2 } -- struct literals use '='
//   []Type                                  -- slice type
//   int, float, bool, string               -- primitive types (lowercase)

const EMPTY_PROGRAM: &str = "";

const HELLO_WORLD: &str = r#"
package demo

fn main() {
    let msg = "Hello, World!"
    print(msg)
}
"#;

const SMALL_SOURCE: &str = r#"
package math

fn add(x: int, y: int) int {
    return x + y
}

fn subtract(x: int, y: int) int {
    return x - y
}

fn multiply(x: int, y: int) int {
    return x * y
}

fn clamp(val: int, lo: int, hi: int) int {
    if val < lo {
        return lo
    } elif val > hi {
        return hi
    } else {
        return val
    }
}

fn main() {
    let x = 10
    let y = 20
    let sum  = add(x, y)
    let diff = subtract(x, y)
    let prod = multiply(x, y)
    let c    = clamp(prod, 0, 100)
    print(sum)
    print(diff)
    print(c)
}
"#;

const MEDIUM_SOURCE: &str = r#"
package inventory

struct Item {
    id:    int
    name:  string
    price: float
    qty:   int
}

fn new_item(id: int, name: string, price: float, qty: int) Item {
    return Item { id = id, name = name, price = price, qty = qty }
}

fn total_value(item: Item) float {
    return item.price * item.qty as float
}

fn apply_discount(item: Item, pct: float) Item {
    let discounted = item.price * (1.0 - pct / 100.0)
    return Item { id = item.id, name = item.name, price = discounted, qty = item.qty }
}

fn is_expensive(item: Item, threshold: float) bool {
    return item.price > threshold
}

fn main() {
    let a = new_item(1, "Widget A", 9.99,   100)
    let b = new_item(2, "Widget B", 24.99,  50)
    let c = new_item(3, "Gadget X", 149.99, 10)
    let d = new_item(4, "Gadget Y", 299.99, 5)

    if is_expensive(c, 50.0) {
        let discounted = apply_discount(c, 10.0)
        print(discounted.name)
        print(total_value(discounted))
    }
    if is_expensive(d, 50.0) {
        let discounted = apply_discount(d, 15.0)
        print(discounted.name)
        print(total_value(discounted))
    }
    print(total_value(a))
    print(total_value(b))
}
"#;

const LARGE_SOURCE: &str = r#"
package simulation

struct Vec3 {
    x: float
    y: float
    z: float
}

fn vec3_add(a: Vec3, b: Vec3) Vec3 {
    return Vec3 { x = a.x + b.x, y = a.y + b.y, z = a.z + b.z }
}

fn vec3_scale(v: Vec3, s: float) Vec3 {
    return Vec3 { x = v.x * s, y = v.y * s, z = v.z * s }
}

fn vec3_dot(a: Vec3, b: Vec3) float {
    return a.x * b.x + a.y * b.y + a.z * b.z
}

fn vec3_len_sq(v: Vec3) float {
    return vec3_dot(v, v)
}

struct Particle {
    pos:  Vec3
    vel:  Vec3
    mass: float
}

fn particle_step(p: Particle, gravity: Vec3, dt: float) Particle {
    let accel   = vec3_scale(gravity, 1.0 / p.mass)
    let new_vel = vec3_add(p.vel, vec3_scale(accel, dt))
    let new_pos = vec3_add(p.pos, vec3_scale(new_vel, dt))
    return Particle { pos = new_pos, vel = new_vel, mass = p.mass }
}

fn simulate_step(p0: Particle, p1: Particle, p2: Particle, gravity: Vec3, dt: float) float {
    let r0 = particle_step(p0, gravity, dt)
    let r1 = particle_step(p1, gravity, dt)
    let r2 = particle_step(p2, gravity, dt)
    return vec3_len_sq(r0.pos) + vec3_len_sq(r1.pos) + vec3_len_sq(r2.pos)
}

fn run_simulation(gravity: Vec3, steps: int, dt: float) float {
    let p0 = Particle { pos = Vec3 { x = 0.0, y = 10.0, z = 0.0 }, vel = Vec3 { x = 1.0, y = 0.0, z = 0.0 }, mass = 1.0 }
    let p1 = Particle { pos = Vec3 { x = 5.0, y = 20.0, z = 0.0 }, vel = Vec3 { x = 0.0, y = 2.0, z = 0.0 }, mass = 2.0 }
    let p2 = Particle { pos = Vec3 { x = 2.0, y = 5.0,  z = 3.0 }, vel = Vec3 { x = 1.0, y = 1.0, z = 1.0 }, mass = 0.5 }
    let acc = 0.0
    let i   = 0
    while i < steps {
        let energy = simulate_step(p0, p1, p2, gravity, dt)
        acc = acc + energy
        i   = i + 1
    }
    return acc
}

fn main() {
    let gravity = Vec3 { x = 0.0, y = -9.81, z = 0.0 }
    let result  = run_simulation(gravity, 60, 0.016)
    print(result)

    let g2     = Vec3 { x = 0.0, y = -1.62, z = 0.0 }
    let result2 = run_simulation(g2, 60, 0.016)
    print(result2)
}
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lex_only(source: &str) -> Vec<ubel_stratum::Token> {
    lexer::tokenize(source).expect("lex failed")
}

fn parse_only(tokens: Vec<ubel_stratum::Token>, source: &str) {
    let arena = AstArena::with_capacity(512 * 1024);
    let _ = parser::parse(&arena, tokens, source.to_string());
}

fn lex_and_parse(source: &str) {
    let tokens = lex_only(source);
    parse_only(tokens, source);
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_lex(c: &mut Criterion) {
    let mut g = c.benchmark_group("lex");
    g.warm_up_time(Duration::from_millis(500));
    g.measurement_time(Duration::from_secs(5));

    let cases = [
        ("empty",  EMPTY_PROGRAM),
        ("hello",  HELLO_WORLD),
        ("small",  SMALL_SOURCE),
        ("medium", MEDIUM_SOURCE),
        ("large",  LARGE_SOURCE),
    ];
    for (name, src) in cases {
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_with_input(BenchmarkId::new("tokenize", name), src, |b, s| {
            b.iter(|| lex_only(black_box(s)))
        });
    }
    g.finish();
}

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("parse");
    g.warm_up_time(Duration::from_millis(500));
    g.measurement_time(Duration::from_secs(8));

    let cases = [
        ("empty",  EMPTY_PROGRAM),
        ("hello",  HELLO_WORLD),
        ("small",  SMALL_SOURCE),
        ("medium", MEDIUM_SOURCE),
        ("large",  LARGE_SOURCE),
    ];
    for (name, src) in cases {
        g.throughput(Throughput::Bytes(src.len() as u64));
        let tokens = lex_only(src);
        g.bench_with_input(BenchmarkId::new("parse_only", name), src, |b, s| {
            b.iter(|| parse_only(black_box(tokens.clone()), black_box(s)))
        });
    }
    g.finish();
}

fn bench_pipeline(c: &mut Criterion) {
    let mut g = c.benchmark_group("pipeline");
    g.warm_up_time(Duration::from_millis(500));
    g.measurement_time(Duration::from_secs(8));

    let cases = [
        ("empty",  EMPTY_PROGRAM),
        ("hello",  HELLO_WORLD),
        ("small",  SMALL_SOURCE),
        ("medium", MEDIUM_SOURCE),
        ("large",  LARGE_SOURCE),
    ];
    for (name, src) in cases {
        g.throughput(Throughput::Bytes(src.len() as u64));
        g.bench_with_input(BenchmarkId::new("lex_and_parse", name), src, |b, s| {
            b.iter(|| lex_and_parse(black_box(s)))
        });
    }
    g.finish();
}

criterion_group!(parser_benches, bench_lex, bench_parse, bench_pipeline);
criterion_main!(parser_benches);
