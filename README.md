# Ubel Stratum Programming Language

Quantum-Ready Multi-Tier Systems Language

## Phase 1: Lexer & Parser (Current)

Building the foundation with:
- ✅ Logos-based lexer (1000+ MB/s target)
- ✅ LALRPOP parser
- ✅ Tree-walking interpreter
- ❌ NO LLVM (deferred to v2.0)

## Quick Start
```bash
# Build
cargo build --release

# Tokenize a file
./target/release/stratc lex examples/basic/hello.strat

# Run benchmarks
cargo bench

# Run tests
cargo test
```

## Project Structure
```
src/
├── lexer/          # Tokenization
├── parser/         # AST generation
├── semantic/       # Type checking
├── interpreter/    # Tree-walking interpreter
└── tier_analysis/  # Multi-tier memory analysis
```

## Performance Targets

- Lexer: 1000-1200 MB/s
- Parser: 500+ MB/s
- Interpreter: 100K ops/sec (baseline)

## License

TBD (MIT or Apache 2.0)
