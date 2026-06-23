// src/main.rs
//! Ubel Stratum Compiler CLI — stratc

mod lexer;
mod error_management;
mod ast;
mod parser;
mod sema;
mod interpreter;

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
    /// Tokenize a .ubl file and show the token stream.
    Lex {
        file: PathBuf,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Parse a .ubl file and show top-level item count.
    Parse { file: PathBuf },
    /// Run name resolution, type inference, and tier checking.
    Check { file: PathBuf },
    /// Run a .ubl file through the tree-walking interpreter.
    Run {
        file: PathBuf,
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.quiet { Logger::disable(); }

    let exit_code = match cli.command {
        Commands::Lex   { file, verbose } => handle_lex(file, verbose),
        Commands::Parse { file }          => handle_parse(file),
        Commands::Check { file }          => handle_check(file),
        Commands::Run   { file, args }    => handle_run(file, args),
    };
    std::process::exit(exit_code);
}

fn read_source(file: &PathBuf) -> Result<String, i32> {
    fs::read_to_string(file).map_err(|e| {
        Logger::error(&format!("failed to read file: {}", e));
        1
    })
}

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

fn handle_parse(file: PathBuf) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    match parser::parse(&arena, tokens, source) {
        Ok(program) => {
            Logger::info(&format!("✅ {} top-level items", program.items.len()));
            0
        }
        Err(em) => { Logger::error("❌ parse failed:"); em.report_all(); 1 }
    }
}

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

fn handle_run(file: PathBuf, _args: Vec<String>) -> i32 {
    let source = match read_source(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    // Arena must outlive the interpreter (which stores &'ast references).
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    let program = match parser::parse(&arena, tokens, source.clone()) {
        Ok(p)   => p,
        Err(em) => { Logger::error("❌ parse failed:"); em.report_all(); return 1; }
    };
    // Run sema first — catch type and tier errors before executing.
    if let Err(em) = sema::analyse(&program, &arena, source) {
        Logger::error("❌ type errors — fix before running:");
        em.report_all();
        return 1;
    }
    // FIX: pass &arena so the interpreter can re-parse interpolated string
    // expressions at runtime using the same persistent arena.
    let mut interp = interpreter::Interpreter::new(&arena);
    match interp.run_program(&program) {
        Ok(())   => 0,
        Err(msg) => { eprintln!("\x1b[31mruntime error:\x1b[0m {}", msg); 1 }
    }
    }
