# Ubel Stratum

**Quantum-Ready Multi-Tier Systems Language**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-concept-orange.svg)]()
[![Built With](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

> **"The right memory model for every function."**

---

## ⚠️ Current Status: Concept & Design Phase

**Ubel Stratum is currently in the design and conceptual phase.** We're building the language specification, memory management model, and syntax before implementation begins.

**Development Roadmap:**
1. ✅ **Phase 1**: Core design & memory management model (Current)
2. 🔄 **Phase 2**: Tree-walking interpreter in Rust (Planned - Not Started Yet)
3. 📋 **Phase 3**: LLVM backend (Future)
4. 📋 **Phase 4**: Production compiler (Future)

**Why tree-walking interpreter first?**
- Validate memory management concepts
- Test tier system interactions
- Prove inter-tier communication patterns
- Fast iteration on language design
- Build confidence before LLVM complexity

**Implementation Status:**
- **Current**: Design and specification phase
- **Next**: Tree-walking interpreter (not started - waiting for other projects to be completed)
- **Building in**: Rust (but implementation hasn't begun yet)

**Note:** Syntax and features are subject to change as we refine the design.

---

## 🚀 What is Ubel Stratum?

Ubel Stratum is a systems programming language that lets you choose the **right memory management strategy for each function**:

- **HIGH Tier** (`@tier(high)`): Garbage collected (like Go/Java/C#)
- **MID Tier** (`@tier(mid)`): Arena allocated (like game engines)
- **LOW Tier** (`@tier(low)`): Manual ownership (like Rust)

### Why Tier-Based Memory?

Most languages force ONE memory model for the entire program:

| Language | Memory Model | Good For | Bad For |
|----------|-------------|----------|---------|
| Go/Java/C# | Garbage Collection | Business logic | Performance-critical code |
| Rust | Ownership | Performance | Rapid prototyping |
| C/C++ | Manual | Everything (with pain) | Safety |

**Ubel lets you choose per-function:**

```strat
// HIGH tier: GC for business logic (easy)
@tier(high)
async fn handle_request(req: Request) Task<Response>! {
    let user = await authenticate(req.token)?
    let data = await fetch_data(user.id)?
    return Response.ok(data)
}

// MID tier: Arena for fast parsing (zero-copy)
@tier(mid)
fn parse_json(input: string) JsonView {
    with arena(1MB) {
        let tokens = tokenize(input)
        let ast = parse(tokens)
        return JsonView.from_arena(ast)
    }  // Arena freed instantly!
}

// LOW tier: Manual for systems code (maximum control)
@tier(low)
fn process_packet(packet: &[u8]) Result<(), Error>! {
    let header = parse_header(packet)?
    let mut buffer = List.with_capacity(header.size)
    // ... manual memory management
    return Ok(())
}
```

---

## 🎯 Key Features

### 1. **C#-Style Collections** (Familiar Naming)

We use C#'s clear, self-explanatory collection names:

```strat
// List<T> - Dynamic array (not Vec!)
let mut numbers = List.new()
numbers.push(1)
numbers.push(2)
numbers.push(3)

// With type annotation
let mut items = List<int>.new()

// Literal syntax
let numbers = [1, 2, 3, 4, 5]
let first = numbers[0]

// Dictionary<K, V> - Hash map (not HashMap!)
let mut ages = Dictionary.new()
ages.insert("Alice", 30)
ages.insert("Bob", 25)

// With type annotation
let mut scores = Dictionary<string, int>.new()

// Access methods (using . for everything)
if let Some(age) = ages.get("Alice") {
    println($"Alice is {age} years old")
}

// Check existence
if ages.contains_key("Bob") {
    println("Bob exists")
}

// Iterate
for (name, age) in ages {
    println($"{name}: {age}")
}
```

**Why List/Dictionary instead of Vec/HashMap?**
- More self-explanatory for developers coming from C#/Java/TypeScript
- "List" clearly means "a list of things"
- "Dictionary" clearly means "key-value lookup"
- Consistent with C# conventions

### 2. **Dynamic Objects** (JavaScript/C# Style)

Create objects on-the-fly without defining structs:

```strat
// Anonymous objects (like C#)
let person = {
    name = "Alice",
    age = 30,
    greet = fn() {
        println($"Hello, I'm {self.name}")
    }
}

person.greet()  // Prints: Hello, I'm Alice
println(person.age)  // 30

// Nested objects
let config = {
    server = {
        host = "localhost",
        port = 8080
    },
    database = {
        url = "postgres://localhost/mydb",
        pool_size = 10
    }
}

println(config.server.host)  // localhost
println(config.database.pool_size)  // 10

// Modify properties
let mut user = {
    name = "Bob",
    score = 0
}
user.score = 100
user.level = 5  // Can add new properties dynamically!

// Arrays of objects
let users = [
    { id = 1, name = "Alice", active = true },
    { id = 2, name = "Bob", active = false },
    { id = 3, name = "Charlie", active = true }
]

// Filter and map
let active_names = users
    .where(fn(u) u.active)
    .map(fn(u) u.name)
```

### 3. **Structs** (When You Need Types)

For type safety and performance, use structs:

```strat
// Basic struct
struct Point {
    x: int,
    y: int
}

// Create instance
let p = Point { x = 10, y = 20 }
println($"Point: ({p.x}, {p.y})")

// Struct with methods
struct Rectangle {
    width: int,
    height: int
    
    pub fn new(w: int, h: int) Rectangle {
        return Rectangle { width = w, height = h }
    }
    
    pub fn area(self) int {
        return self.width * self.height
    }
    
    pub fn is_square(self) bool {
        return self.width == self.height
    }
}

let rect = Rectangle.new(10, 5)
println($"Area: {rect.area()}")  // Area: 50

if rect.is_square() {
    println("It's a square!")
}

// Generic structs
struct Container<T> {
    value: T
    
    pub fn new(val: T) Container<T> {
        return Container { value = val }
    }
    
    pub fn get(self) &T {
        return &self.value
    }
    
    pub fn set(mut self, val: T) {
        self.value = val
    }
}

let int_box = Container.new(42)
let string_box = Container.new("hello")

println(int_box.get())  // 42
println(string_box.get())  // hello

// Edge structs (manual memory - LOW tier)
edge struct Node {
    value: int,
    next: Node?  // Optional pointer
    
    pub fn new(val: int) Node {
        return Node { value = val, next = None }
    }
}
```

**When to use structs vs dynamic objects:**
- **Structs**: Type safety, performance, large codebases, public APIs
- **Dynamic objects**: Rapid prototyping, JSON-like data, config files, small scripts

### 4. **No `::` - Just `.` for Everything**

Unlike Rust, Ubel uses `.` consistently for all access:

```strat
// Module/namespace access
summon std.collections.List
summon std.collections.Dictionary
summon std.io.File

// Type-level functions (constructors, static methods)
let list = List.new()           // Constructor
let capacity_list = List.with_capacity(100)
let file = File.open("data.txt")

// Instance methods
list.push(1)
list.push(2)
let count = list.len()

// It's always . - never ::
// Compiler knows from context what you mean
```

**Why no `::`?**
- Simpler - one operator to remember
- Familiar to C#/Java/Python/JavaScript developers
- Context makes it obvious (type vs instance)
- Less visual noise

### 5. **Zero-Copy Inter-Tier Communication**

Three patterns for passing data between tiers:

**Callback Pattern** (Single result):
```strat
@tier(high)
fn handle_request(req: Request) Response {
    parse_json_with(req.body, fn(json) {
        // Read from arena without copying
        let user_id = json.get("user_id").as_int()
        return process_user(user_id)
    })  // Arena freed here
}

@tier(mid)
fn parse_json_with<F>(input: string, callback: F) Response
    where F: FnOnce(&JsonView) Response
{
    with arena(1MB) {
        let json = parse(input)
        return callback(&json)  // Zero-copy!
    }
}
```

**Iterator Pattern** (Streaming):
```strat
@tier(high)
fn process_batch(items: List<Item>) {
    // Stream from arena without copying
    for result in transform_items(items) {
        save_to_database(result)
    }  // Arena freed when iteration completes
}

@tier(mid)
fn transform_items(items: &List<Item>) ArenaIterator<Result> {
    with arena(10MB) {
        let results = items.map(fn(item) transform(item))
        return ArenaIterator.new(results, arena)
    }
}
```

**View Pattern** (Read-only access):
```strat
@tier(high)
fn parse_config(path: string) Config {
    let view = parse_toml(path)
    
    // Read without copying
    let host = view.get("host").as_string()
    let port = view.get("port").as_int()
    
    // Explicit copy only what's needed
    return Config { host = host.to_owned(), port = port }
}  // Arena freed, only Config is GC-allocated
```

### 6. **Readable Lifetime Syntax**

Instead of Rust's cryptic `<'a>`, Ubel uses clear syntax:

```strat
// Simple cases: Inferred (no annotation needed)
fn first(x: &str) string {
    return x
}

// Complex cases: Named lifetimes
fn longest[lifetime L](x: &L str, y: &L str) &L str {
    if x.len() > y.len() { return x } else { return y }
}

// Very complex: Lifetime constraints
fn advanced[lifetime L, lifetime M where M outlives L](
    x: &L Data,
    y: &M Config
) &L Result {
    return process(x, y)
}
```

### 7. **Modern Async/Await**

```strat
// HIGH tier: Async allowed (GC keeps data alive)
@tier(high)
async fn fetch_user(id: int) Task<User>! {
    let response = await http_get($"/users/{id}")?
    let user = await parse_json(response.body)?
    return user
}

// Concurrent operations
@tier(high)
async fn fetch_multiple(ids: List<int>) Task<List<User>>! {
    let tasks = ids.map(fn(id) fetch_user(id))
    let users = await Task.all(tasks)
    return users
}
```

### 8. **Powerful Pattern Matching**

```strat
match response {
    Ok(data) where data.status == 200 => {
        process_success(data)
    }
    Ok(data) => {
        log_warning($"Unexpected status: {data.status}")
    }
    Err(NetworkError(extract { code, message })) => {
        log_error($"Network error {code}: {message}")
    }
    Err(e) => {
        log_error($"Unknown error: {e}")
    }
}
```

### 9. **Extract Keyword** (Destructuring)

```strat
// Tuple destructuring
let point = (10, 20)
extract (x, y) = point

// List destructuring
let numbers = [1, 2, 3, 4, 5]
extract [first, second, ...rest] = numbers

// Object destructuring
let user = { id = 1, name = "Alice", email = "alice@example.com" }
extract { id, name } = user

// Nested destructuring
let data = {
    user = { name = "Alice", age = 30 },
    status = "active"
}
extract { user = { name, age }, status } = data

// In function parameters
fn process_point(extract (x, y): (int, int)) {
    println($"Point: ({x}, {y})")
}
```

### 10. **Pipe Operator** (Functional Composition)

```strat
// Instead of nested calls
let result = save(transform(validate(parse(data))))

// Use pipe operator
let result = data
    |> parse
    |> validate
    |> transform
    |> save

// With error propagation
let result = data
    |> parse?
    |> validate?
    |> transform?
    |> save?

// With lambdas
let evens = numbers
    |> filter(fn(x) x > 0)
    |> map(fn(x) x * 2)
    |> filter(fn(x) x % 2 == 0)
```

### 11. **Extension Functions**

```strat
// Extend existing types
extend int {
    fn is_even(self) bool {
        return self % 2 == 0
    }
    
    fn times(self, action: fn(int)) {
        for i in 0..self {
            action(i)
        }
    }
}

// Use them
if 42.is_even() {
    println("Even!")
}

5.times(fn(i) {
    println($"Iteration {i}")
})

// Extend List
extend List<T> {
    fn second(self) T? {
        if self.len() >= 2 {
            return Some(self[1])
        }
        return None
    }
    
    fn last(self) T? {
        if self.len() > 0 {
            return Some(self[self.len() - 1])
        }
        return None
    }
}

let numbers = [1, 2, 3, 4, 5]
println(numbers.second())  // Some(2)
println(numbers.last())    // Some(5)
```

### 12. **RAII with `using`**

```strat
// Automatic cleanup
using let file = File.open("data.txt") {
    let content = file.read()
    process(content)
}  // file.close() called automatically

// Multiple resources
using let db = Database.connect("localhost"),
      let cache = Cache.connect("redis") {
    // Work with both
}  // Both cleaned up in reverse order

// Works with any type that implements Drop
using let lock = mutex.lock() {
    // Critical section
}  // lock released automatically
```

### 13. **Built-in LINQ** (HIGH tier)

```strat
@tier(high)
fn get_active_adults(users: List<User>) List<string> {
    return from user in users
           where user.age >= 18 
             and user.status == "active"
           orderby user.name
           select user.name
}
```

Or use method syntax (works in ALL tiers):
```strat
@tier(mid)
fn get_active_adults(users: List<User>) List<string> {
    return users
        .where(fn(u) u.age >= 18 and u.status == "active")
        .orderby(fn(u) u.name)
        .map(fn(u) u.name)
}
```

---

## 📋 Complete Example: Web API Server

```strat
package api_server

summon std.http
summon std.json
summon std.collections.List
summon std.collections.Dictionary
from database summon [User, Database]

// User struct
struct User {
    id: int,
    name: string,
    email: string,
    age: int,
    
    pub fn new(id: int, name: string, email: string, age: int) User {
        return User { id = id, name = name, email = email, age = age }
    }
    
    pub fn to_json(self) string {
        return $"{{\"id\":{self.id},\"name\":\"{self.name}\",\"email\":\"{self.email}\",\"age\":{self.age}}}"
    }
    
    pub fn from_json(json: &JsonView) User! {
        let id = json.get("id").as_int()?
        let name = json.get("name").as_string()?.to_owned()
        let email = json.get("email").as_string()?.to_owned()
        let age = json.get("age").as_int()?
        return User.new(id, name, email, age)
    }
}

// Entry point - HIGH tier for async
@tier(high)
async fn main() Task<void>! {
    let server = HttpServer.new("0.0.0.0:8080")
    println("Server listening on port 8080")
    
    await server.serve(handle_request)
}

// Request handler - HIGH tier for async I/O
@tier(high)
async fn handle_request(req: Request) Task<Response>! {
    match req.path {
        "/api/users" => await handle_users(req),
        "/api/users/search" => handle_search(req),
        "/api/parse" => handle_parse(req),
        _ => Response.not_found()
    }
}

// User handler - HIGH tier for database
@tier(high)
async fn handle_users(req: Request) Task<Response>! {
    let db = await Database.connect("localhost")?
    
    match req.method {
        "GET" => {
            // Fetch all users
            let users = await db.query("SELECT * FROM users")?
            let json = users.map(fn(u) u.to_json()).join(",")
            return Response.ok($"[{json}]")
        }
        "POST" => {
            // Parse and create user (MID tier - fast!)
            let user = parse_user_from_body(req.body)?
            
            // Save to database
            let id = await db.insert_user(user)?
            
            return Response.created($"Created user {id}")
        }
        _ => Response.method_not_allowed()
    }
}

// Search handler - demonstrates List operations
@tier(high)
fn handle_search(req: Request) Response {
    // Get search params
    let min_age = req.get_param("min_age")?.parse_int()?
    
    // Mock data (in real app, would query database)
    let users = List.from([
        User.new(1, "Alice", "alice@example.com", 30),
        User.new(2, "Bob", "bob@example.com", 25),
        User.new(3, "Charlie", "charlie@example.com", 35),
        User.new(4, "Diana", "diana@example.com", 28),
    ])
    
    // Filter and map (LINQ-style)
    let results = users
        .where(fn(u) u.age >= min_age)
        .orderby(fn(u) u.name)
        .map(fn(u) u.to_json())
        .collect()
    
    let json = results.join(",")
    return Response.ok($"[{json}]")
}

// Parse handler - Uses MID tier for speed
@tier(high)
fn handle_parse(req: Request) Response {
    parse_json_with(req.body, fn(json) {
        // Create dynamic object from JSON
        let data = {
            field_count = json.keys().count(),
            fields = json.keys().collect(),
            sample = json.get("name").as_string().or("N/A")
        }
        
        return Response.ok($"Parsed {data.field_count} fields: {data.sample}")
    })
}

// Parse user from request body
@tier(high)
fn parse_user_from_body(body: string) User! {
    let result = None
    
    parse_json_with(body, fn(json) {
        result = Some(User.from_json(json))
        return Response.ok("")  // Dummy response
    })
    
    return result?
}

// MID tier: Fast JSON parsing
@tier(mid)
fn parse_json_with<F>(input: string, callback: F) Response
    where F: FnOnce(&JsonView) Response
{
    with arena(1MB) {
        let tokens = tokenize(input)
        let ast = parse_tokens(tokens)
        let view = JsonView.from_arena(ast)
        
        return callback(&view)  // Zero-copy!
    }  // Arena freed
}

// MID tier: Tokenization
@tier(mid)
fn tokenize(input: string) List<Token> {
    // Allocated in caller's arena
    let tokens = List.new()
    let mut pos = 0
    
    while pos < input.len() {
        let token = read_token(input, pos)
        tokens.push(token)
        pos = token.end
    }
    
    return tokens
}

// Helper structs
struct Token {
    kind: TokenKind,
    value: string,
    start: int,
    end: int
}

enum TokenKind {
    LeftBrace,
    RightBrace,
    String,
    Number,
    Colon,
    Comma
}

struct JsonView {
    // Implementation details...
}
```

---

## 🎨 More Examples

### Working with Collections

```strat
@tier(high)
fn collection_examples() {
    // List operations
    let mut numbers = List.new()
    numbers.push(1)
    numbers.push(2)
    numbers.push(3)
    
    println($"Count: {numbers.len()}")  // Count: 3
    
    // List from array
    let names = List.from(["Alice", "Bob", "Charlie"])
    
    // List with capacity
    let mut items = List.with_capacity(100)
    for i in 0..100 {
        items.push(i)
    }
    
    // Dictionary operations
    let mut ages = Dictionary.new()
    ages.insert("Alice", 30)
    ages.insert("Bob", 25)
    ages.insert("Charlie", 35)
    
    // Get value
    if let Some(age) = ages.get("Alice") {
        println($"Alice is {age}")
    }
    
    // Check key exists
    if ages.contains_key("Bob") {
        println("Bob is in the dictionary")
    }
    
    // Iterate
    for (name, age) in ages {
        println($"{name}: {age}")
    }
    
    // Remove
    ages.remove("Charlie")
    
    // Dictionary from literal
    let scores = {
        "Alice" = 100,
        "Bob" = 85,
        "Charlie" = 92
    }
}
```

### Struct vs Dynamic Object

```strat
// When to use struct: Type safety, performance
struct Point {
    x: int,
    y: int
    
    pub fn distance_from_origin(self) float {
        return ((self.x * self.x + self.y * self.y) as float).sqrt()
    }
}

@tier(high)
fn struct_example() {
    let p1 = Point { x = 3, y = 4 }
    let p2 = Point { x = 6, y = 8 }
    
    println($"Distance: {p1.distance_from_origin()}")
}

// When to use dynamic object: Flexibility, JSON-like data
@tier(high)
fn dynamic_example() {
    // Quick prototyping
    let config = {
        debug = true,
        port = 8080,
        allowed_origins = ["http://localhost:3000"]
    }
    
    if config.debug {
        println($"Debug mode on port {config.port}")
    }
    
    // JSON response
    let response = {
        status = "success",
        data = {
            users = [
                { id = 1, name = "Alice" },
                { id = 2, name = "Bob" }
            ],
            count = 2
        }
    }
}
```

---

## 🏗️ Memory Management Details

### The Three Tiers

#### HIGH Tier: Garbage Collection
- **Algorithm**: Concurrent tri-color mark-and-sweep (like Go)
- **Pauses**: <500μs (sub-millisecond)
- **Overhead**: ~1-5% CPU
- **Use For**: Business logic, APIs, web services
- **Collections**: List, Dictionary, etc. are GC-managed

```strat
@tier(high)
fn business_logic(data: Data) Result {
    let items = List.new()      // GC allocated
    items.push(transform(data))  // GC allocated
    return process(items)        // GC managed
}
```

#### MID Tier: Arena Allocation
- **Algorithm**: Monotonic bump allocator
- **Cleanup**: Instant (reset arena)
- **Performance**: 2-10x faster than malloc
- **Use For**: Request handlers, parsers, batch processing

```strat
@tier(mid)
fn parse_request(body: string) ParsedData {
    with arena(1MB) {
        let tokens = tokenize(body)      // Arena allocated
        let ast = parse(tokens)           // Arena allocated
        return extract_data(ast).to_gc()  // Copy to GC
    }  // Arena freed - instant!
}
```

#### LOW Tier: Manual Ownership
- **Algorithm**: Rust-style ownership + borrow checker
- **Safety**: Compile-time guarantees
- **Performance**: Zero overhead
- **Use For**: Systems code, drivers, hot paths

```strat
@tier(low)
fn process_buffer[lifetime L](buf: &mut L [u8]) usize {
    let mut written = 0
    for byte in buf {
        *byte = transform(*byte)
        written += 1
    }
    return written
}
```

---

## 🛠️ Building Ubel Stratum

### Current Status

**Phase 1: Design (Current)**
- ✅ Tier system design
- ✅ Memory management model
- ✅ Syntax design
- ✅ Collection naming (List/Dictionary)
- ✅ Dynamic objects
- 🔄 Standard library design

**Phase 2: Interpreter (Planned - Not Started)**
- Lexer and parser
- Type system
- Borrow checker (LOW tier)
- Arena allocator (MID tier)
- GC implementation (HIGH tier)
- Inter-tier communication

**Status**: Waiting for other projects to be completed before starting implementation.

**Built in**: Rust (but implementation hasn't begun yet)

### Contributing

**We welcome contributions and suggestions!**

**How to contribute:**
- 📝 Design feedback on language features
- 💡 Syntax suggestions
- 📖 Documentation improvements
- 🐛 Future implementation help (when we start)

**Discussion topics:**
- Collection naming conventions
- Syntax clarity and consistency
- Standard library features
- Real-world use cases

---

## 📚 Documentation

- [Memory Management Design](docs/memory-management.md)
- [Tier System Guide](docs/tier-system.md)
- [Lifetime Syntax](docs/lifetimes.md)
- [Async/Await](docs/async.md)
- [EBNF Grammar](docs/ubel.ebnf)

---

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## 🌟 Why "Ubel Stratum"?

**Ubel** (German: "evil/bad") + **Stratum** (Latin: "layer")

The name reflects the language's multi-layered approach to memory management. The "ubel" part is tongue-in-cheek - choosing between memory models shouldn't be evil, it should be empowering.

---

**Ubel Stratum: The right memory model for every function.** 🚀
