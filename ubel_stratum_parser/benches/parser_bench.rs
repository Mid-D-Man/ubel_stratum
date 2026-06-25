// ubel_stratum_parser/benches/parser_bench.rs
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

use ubel_stratum::ast::arena::AstArena;
use ubel_stratum::lexer;
use ubel_stratum_parser as parser;

// ── Source fixtures ───────────────────────────────────────────────────────────

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

fn add(a: Int, b: Int) -> Int {
    return a + b
}

fn subtract(a: Int, b: Int) -> Int {
    return a - b
}

fn multiply(a: Int, b: Int) -> Int {
    return a * b
}

fn main() {
    let x = 10
    let y = 20
    let sum  = add(x, y)
    let diff = subtract(x, y)
    let prod = multiply(x, y)
    print(sum)
    print(diff)
    print(prod)
}
"#;

const MEDIUM_SOURCE: &str = r#"
package inventory

struct Item {
    id:    Int
    name:  String
    price: Float
    qty:   Int
}

fn new_item(id: Int, name: String, price: Float, qty: Int) -> Item {
    return Item { id: id, name: name, price: price, qty: qty }
}

fn total_value(item: Item) -> Float {
    return item.price * item.qty as Float
}

fn apply_discount(item: Item, pct: Float) -> Item {
    let discounted = item.price * (1.0 - pct / 100.0)
    return Item { id: item.id, name: item.name, price: discounted, qty: item.qty }
}

fn find_expensive(items: [Item], threshold: Float) -> [Item] {
    let result = []
    for item in items {
        if item.price > threshold {
            result.push(item)
        }
    }
    return result
}

fn main() {
    let items = [
        new_item(1, "Widget A", 9.99,  100),
        new_item(2, "Widget B", 24.99, 50),
        new_item(3, "Gadget X", 149.99, 10),
        new_item(4, "Gadget Y", 299.99, 5),
    ]

    let expensive = find_expensive(items, 50.0)
    for e in expensive {
        let discounted = apply_discount(e, 10.0)
        print(discounted.name)
        print(total_value(discounted))
    }
}
"#;

const LARGE_SOURCE: &str = r#"
package simulation

struct Vec3 {
    x: Float
    y: Float
    z: Float
}

fn vec3_add(a: Vec3, b: Vec3) -> Vec3 {
    return Vec3 { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z }
}

fn vec3_scale(v: Vec3, s: Float) -> Vec3 {
    return Vec3 { x: v.x * s, y: v.y * s, z: v.z * s }
}

fn vec3_dot(a: Vec3, b: Vec3) -> Float {
    return a.x * b.x + a.y * b.y + a.z * b.z
}

fn vec3_len_sq(v: Vec3) -> Float {
    return vec3_dot(v, v)
}

struct Particle {
    pos:  Vec3
    vel:  Vec3
    mass: Float
}

fn particle_update(p: Particle, dt: Float) -> Particle {
    let new_pos = vec3_add(p.pos, vec3_scale(p.vel, dt))
    return Particle { pos: new_pos, vel: p.vel, mass: p.mass }
}

fn particle_apply_force(p: Particle, force: Vec3, dt: Float) -> Particle {
    let accel   = vec3_scale(force, 1.0 / p.mass)
    let new_vel = vec3_add(p.vel, vec3_scale(accel, dt))
    return Particle { pos: p.pos, vel: new_vel, mass: p.mass }
}

fn simulate(particles: [Particle], steps: Int, dt: Float) -> [Particle] {
    let gravity = Vec3 { x: 0.0, y: -9.81, z: 0.0 }
    let state   = particles
    let i = 0
    while i < steps {
        let j    = 0
        let next = []
        while j < state.len() {
            let p = state[j]
            let p2 = particle_apply_force(p, gravity, dt)
            let p3 = particle_update(p2, dt)
            next.push(p3)
            j = j + 1
        }
        state = next
        i = i + 1
    }
    return state
}

fn main() {
    let particles = [
        Particle { pos: Vec3 { x: 0.0, y: 10.0, z: 0.0 }, vel: Vec3 { x: 1.0, y: 0.0, z: 0.0 }, mass: 1.0 },
        Particle { pos: Vec3 { x: 5.0, y: 20.0, z: 0.0 }, vel: Vec3 { x: 0.0, y: 2.0, z: 0.0 }, mass: 2.0 },
        Particle { pos: Vec3 { x: 2.0, y:  5.0, z: 3.0 }, vel: Vec3 { x: 1.0, y: 1.0, z: 1.0 }, mass: 0.5 },
    ]
    let result = simulate(particles, 100, 0.016)
    for r in result {
        print(vec3_len_sq(r.pos))
    }
}
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lex_only(source: &str) -> Vec<ubel_stratum::Token> {
    lexer::tokenize(source).expect("lex failed")
}

fn parse_with_arena(tokens: Vec<ubel_stratum::Token>, source: &str) {
    let arena = AstArena::with_capacity(512 * 1024);
    let _ = parser::parse(&arena, tokens, source.to_string());
}

fn lex_and_parse(source: &str) {
    let tokens = lex_only(source);
    parse_with_arena(tokens, source);
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
        // Pre-lex so we're benchmarking only the parser pass
        let tokens = lex_only(src);
        g.bench_with_input(BenchmarkId::new("parse_only", name), src, |b, s| {
            b.iter(|| parse_with_arena(black_box(tokens.clone()), black_box(s)))
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
