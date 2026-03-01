
criterion_main!(benches);use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::time::Duration;
use ubel_stratum::lexer::tokenize;

const SMALL_SOURCE: &str = r#"
fn main() {
    let x = 42
    println("Hello, World!")
}
"#;

const MEDIUM_SOURCE: &str = r#"
package game_engine.physics

summon std.math
from std.collections summon [List, Dictionary]

struct Vector2 {
    x: float
    y: float

    pub fn new(x: float, y: float) Vector2 {
        return Vector2 { x: x, y: y }
    }

    pub fn magnitude(self) float {
        return Math.sqrt(self.x * self.x + self.y * self.y)
    }
}

async fn fetch_data(url: string) Task<string>! {
    let response = await http_get(url)
    if response.status != 200 {
        fail $"Request failed: {response.status}"
    }
    return response.body
}

fn main() {
    let v = Vector2.new(3.0f, 4.0f)
    let mag = v.magnitude()
    println($"Magnitude: {mag}")
}
"#;

const LARGE_SOURCE: &str = r#"
package api_server.handlers

summon std.http
summon std.json
from database summon [User, Session, Repository]
from middleware summon [auth, logging, rate_limit]

pub struct UserHandler {
    repo: Repository<User>

    pub fn new(repo: Repository<User>) UserHandler {
        return UserHandler { repo: repo }
    }

    pub async fn get_user(self, id: int) Task<User>! {
        let user = await self.repo.find_by_id(id)?
        return user
    }

    pub async fn create_user(self, data: UserData) Task<User>! {
        if data.email.is_empty() {
            fail "Email is required"
        }
        let user = User {
            id = 0,
            email = data.email,
            name = data.name,
            created_at = DateTime.now()
        }
        let saved = await self.repo.save(user)?
        return saved
    }

    pub async fn list_users(self, limit: int = 100) Task<List<User>>! {
        let users = await self.repo.find_all(limit)?
        return users
    }
}

enum Status {
    Active = 1,
    Inactive = 2,
    Suspended = 3
}

@tier(mid)
fn process_batch(users: []User) Result! {
    with arena(10MB) {
        for user in users {
            let validated = validate(user) or continue
            let processed = transform(validated)
            save(processed)
        }
    }
}

fn main() {
    let handler = UserHandler.new(get_repo())
    let users = await handler.list_users(50)
    match users {
        Ok(data) => println($"Found {data.len()} users"),
        Err(e)   => println($"Error: {e}")
    }
}
"#;

// ── keyword lookup bench uses the public helper from keywords module ──────────
pub mod keywords {
    use ubel_stratum::lexer::TokenType;
    use ubel_stratum::lexer::keywords::get_keyword;

    pub fn lookup_mix() {
        let _ = get_keyword("fn");
        let _ = get_keyword("let");
        let _ = get_keyword("async");
        let _ = get_keyword("struct");
        let _ = get_keyword("notakeyword");
        let _ = get_keyword("return");
        let _ = get_keyword("match");
        let _ = get_keyword("xyzzy");
    }
}

fn lexer_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");

    // Hard caps so no single bench can run the runner out of time.
    // small: generous — it's fast
    // medium: 8 s is plenty
    // large: 10 s max, 50 samples instead of 100
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);
    group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("small"),
        &SMALL_SOURCE,
        |b, input| b.iter(|| tokenize(black_box(input))),
    );

    group.measurement_time(Duration::from_secs(8));
    group.sample_size(100);
    group.throughput(Throughput::Bytes(MEDIUM_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("medium"),
        &MEDIUM_SOURCE,
        |b, input| b.iter(|| tokenize(black_box(input))),
    );

    // Large gets capped: 10 s wall time, 50 samples.
    // This prevents criterion from estimating "need 45 minutes" and getting killed.
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);
    group.throughput(Throughput::Bytes(LARGE_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("large"),
        &LARGE_SOURCE,
        |b, input| b.iter(|| tokenize(black_box(input))),
    );

    group.finish();
}

fn keyword_lookup_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyword_lookup");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    group.bench_function("mixed_8_lookups", |b| {
        b.iter(|| black_box(keywords::lookup_mix()))
    });

    group.finish();
}

criterion_group!(benches, lexer_benchmarks, keyword_lookup_bench);
criterion_main!(benches);
