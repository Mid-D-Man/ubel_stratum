// src/main.rs — no change needed.
// The binary is always compiled with default features (which includes parser),
// so `stratc run`, `stratc check`, etc. all still work.
// If someone does `cargo build --no-default-features` they'll get a compile
// error on the parser imports, which is correct — the CLI needs the parser.
// For the binary you always want default features.

mod lexer;
mod error_management;
mod ast;
mod sema;
mod interpreter;

#[cfg(feature = "parser")]
mod parser;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use error_management::Logger;

#[derive(Parser)]
#[command(name = "stratc")]
#[command(about = "Ubel Stratum Compiler — Quantum-Ready Multi-Tier Language")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    Lex   { file: PathBuf, #[arg(short, long)] verbose: bool },
    Parse { file: PathBuf },
    Check { file: PathBuf },
    Run   { file: PathBuf, args: Vec<String> },
}

fn main() {
    let cli = Cli::parse();
    if cli.quiet { Logger::disable(); }

    #[cfg(not(feature = "parser"))]
    {
        eprintln!("stratc was built without the parser feature. Rebuild with default features.");
        std::process::exit(1);
    }

    #[cfg(feature = "parser")]
    {
        let exit_code = match cli.command {
            Commands::Lex   { file, verbose } => handle_lex(file, verbose),
            Commands::Parse { file }          => handle_parse(file),
            Commands::Check { file }          => handle_check(file),
            Commands::Run   { file, args }    => handle_run(file, args),
        };
        std::process::exit(exit_code);
    }
}

fn read_source(file: &PathBuf) -> Result<String, i32> {
    fs::read_to_string(file).map_err(|e| {
        Logger::error(&format!("failed to read file: {}", e));
        1
    })
}

#[cfg(feature = "parser")]
fn handle_lex(file: PathBuf, verbose: bool) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    match lexer::tokenize(&source) {
        Ok(tokens) => {
            if verbose {
                println!("\n{} tokens:", tokens.len());
                println!("{:-<80}", "");
                for (i, tok) in tokens.iter().enumerate() {
                    println!("{:4} | {:?}", i, tok);
                }
                println!("{:-<80}", "");
            } else {
                Logger::info(&format!("✅ {} tokens", tokens.len()));
            }
            0
        }
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); 1 }
    }
}

#[cfg(feature = "parser")]
fn handle_parse(file: PathBuf) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    match parser::parse(&arena, tokens, source) {
        Ok(program) => { Logger::info(&format!("✅ {} top-level items", program.items.len())); 0 }
        Err(em)     => { Logger::error("❌ parse failed:"); em.report_all(); 1 }
    }
}

#[cfg(feature = "parser")]
fn handle_check(file: PathBuf) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    let program = match parser::parse(&arena, tokens, source.clone()) {
        Ok(p)   => p,
        Err(em) => { Logger::error("❌ parse failed:"); em.report_all(); return 1; }
    };
    match sema::analyse(&program, &arena, source) {
        Ok(_)   => { Logger::info("✅ check passed — no errors"); 0 }
        Err(em) => { Logger::error("❌ check failed:"); em.report_all(); 1 }
    }
}

#[cfg(feature = "parser")]
fn handle_run(file: PathBuf, _args: Vec<String>) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    let program = match parser::parse(&arena, tokens, source.clone()) {
        Ok(p)   => p,
        Err(em) => { Logger::error("❌ parse failed:"); em.report_all(); return 1; }
    };
    if let Err(em) = sema::analyse(&program, &arena, source) {
        Logger::error("❌ type errors:");
        em.report_all();
        return 1;
    }
    let mut interp = interpreter::Interpreter::new(&arena);
    match interp.run_program(&program) {
        Ok(())   => 0,
        Err(msg) => { eprintln!("\x1b[31mruntime error:\x1b[0m {}", msg); 1 }
    }
        }
