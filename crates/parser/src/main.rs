// ubel_stratum_parser/src/main.rs
// stratc — Ubel Stratum compiler CLI.
// Lives in ubel_stratum_parser so the parser is always available here.

use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

use ubel_stratum_parser::{ast, error_management, lexer, parse};
use ubel_stratum::{interpreter, sema};

#[derive(Parser)]
#[command(name = "stratc")]
#[command(about = "Ubel Stratum Compiler — Quantum-Ready Multi-Tier Language")]
#[command(version  = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, global = true, help = "Suppress colour output")]
    no_color: bool,
    #[arg(short, long, global = true, help = "Suppress informational output")]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenise a source file and print the token stream
    Lex {
        file: PathBuf,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Parse a source file and print top-level item count
    Parse { file: PathBuf },
    /// Type-check a source file without running it
    Check { file: PathBuf },
    /// Compile and run a source file
    Run {
        file: PathBuf,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.quiet {
        error_management::Logger::disable();
    }
    let code = match cli.command {
        Commands::Lex   { file, verbose } => cmd_lex(file, verbose),
        Commands::Parse { file }          => cmd_parse(file),
        Commands::Check { file }          => cmd_check(file),
        Commands::Run   { file, args }    => cmd_run(file, args),
    };
    std::process::exit(code);
}

fn read(file: &PathBuf) -> Result<String, i32> {
    fs::read_to_string(file).map_err(|e| {
        error_management::Logger::error(&format!("cannot read '{}': {e}", file.display()));
        1
    })
}

fn cmd_lex(file: PathBuf, verbose: bool) -> i32 {
    let src = match read(&file) { Ok(s) => s, Err(c) => return c };
    match lexer::tokenize(&src) {
        Ok(tokens) => {
            if verbose {
                println!("\n{} tokens:", tokens.len());
                println!("{:-<80}", "");
                for (i, t) in tokens.iter().enumerate() { println!("{i:4} | {t:?}"); }
                println!("{:-<80}", "");
            } else {
                error_management::Logger::info(&format!("✅ {} tokens", tokens.len()));
            }
            0
        }
        Err(em) => { error_management::Logger::error("❌ lex failed:"); em.report_all(); 1 }
    }
}

fn cmd_parse(file: PathBuf) -> i32 {
    let src    = match read(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&src) {
        Ok(t)   => t,
        Err(em) => { error_management::Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    match parse(&arena, tokens, src) {
        Ok(p)   => { error_management::Logger::info(&format!("✅ {} items", p.items.len())); 0 }
        Err(em) => { error_management::Logger::error("❌ parse failed:"); em.report_all(); 1 }
    }
}

fn cmd_check(file: PathBuf) -> i32 {
    let src    = match read(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&src) {
        Ok(t)   => t,
        Err(em) => { error_management::Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena   = ast::arena::AstArena::with_capacity(256 * 1024);
    let program = match parse(&arena, tokens, src.clone()) {
        Ok(p)   => p,
        Err(em) => { error_management::Logger::error("❌ parse failed:"); em.report_all(); return 1; }
    };
    match sema::analyse(&program, &arena, src) {
        Ok(_)   => { error_management::Logger::info("✅ check passed"); 0 }
        Err(em) => { error_management::Logger::error("❌ check failed:"); em.report_all(); 1 }
    }
}

fn cmd_run(file: PathBuf, _args: Vec<String>) -> i32 {
    let src    = match read(&file) { Ok(s) => s, Err(c) => return c };
    let tokens = match lexer::tokenize(&src) {
        Ok(t)   => t,
        Err(em) => { error_management::Logger::error("❌ lex failed:"); em.report_all(); return 1; }
    };
    let arena   = ast::arena::AstArena::with_capacity(256 * 1024);
    let program = match parse(&arena, tokens, src.clone()) {
        Ok(p)   => p,
        Err(em) => { error_management::Logger::error("❌ parse failed:"); em.report_all(); return 1; }
    };
    if let Err(em) = sema::analyse(&program, &arena, src) {
        error_management::Logger::error("❌ type errors:");
        em.report_all();
        return 1;
    }
    let mut interp = interpreter::Interpreter::new(&arena);
    match interp.run_program(&program) {
        Ok(())   => 0,
        Err(msg) => { eprintln!("\x1b[31mruntime error:\x1b[0m {msg}"); 1 }
    }
      }
