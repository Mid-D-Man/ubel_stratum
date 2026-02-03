use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
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

/**
 * User handler - manages user operations
 */
pub struct UserHandler {
    repo: Repository<User>

    pub fn new(repo: Repository<User>) UserHandler {
        return UserHandler { repo: repo }
    }

    /*!
     * Get user by ID
     * Returns user or error if not found
     */
    pub async fn get_user(self, id: int) Task<User>! {
        let user = await self.repo.find_by_id(id)?
        return user
    }

    pub async fn create_user(self, data: UserData) Task<User>! {
        // Validate input
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
        Err(e) => println($"Error: {e}")
    }
}
"#;

fn lexer_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");

    // Small source
    group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("small"),
        &SMALL_SOURCE,
        |b, input| {
            b.iter(|| tokenize(black_box(input)));
        },
    );

    // Medium source
    group.throughput(Throughput::Bytes(MEDIUM_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("medium"),
        &MEDIUM_SOURCE,
        |b, input| {
            b.iter(|| tokenize(black_box(input)));
        },
    );

    // Large source
    group.throughput(Throughput::Bytes(LARGE_SOURCE.len() as u64));
    group.bench_with_input(
        BenchmarkId::from_parameter("large"),
        &LARGE_SOURCE,
        |b, input| {
            b.iter(|| tokenize(black_box(input)));
        },
    );

    group.finish();
}

fn keyword_lookup_bench(c: &mut Criterion) {
    use ubel_stratum::lexer::keywords;

    c.bench_function("keyword_lookup", |b| {
        b.iter(|| {
            black_box(keywords::get_keyword("fn"));
            black_box(keywords::get_keyword("let"));
            black_box(keywords::get_keyword("async"));
            black_box(keywords::get_keyword("notakeyword"));
        });
    });
}

criterion_group!(benches, lexer_benchmarks, keyword_lookup_bench);
criterion_main!(benches);