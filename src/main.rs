//! Ubel Stratum Compiler CLI

mod lexer;
mod error_management;
mod ast;
mod parser;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use error_management::Logger;

#[derive(Parser)]
#[command(name = "stratc")]
#[command(about = "Ubel Stratum Compiler - Quantum-Ready Multi-Tier Language")]
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
    /// Tokenize a .strat file (show tokens)
    Lex {
        file: PathBuf,
        #[arg(short, long)]
        verbose: bool,
    },
    /// Parse a .strat file (show AST)
    Parse {
        file: PathBuf,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Check syntax and types
    Check { file: PathBuf },
    /// Run a .strat file (interpreter)
    Run {
        file: PathBuf,
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    if cli.quiet { Logger::disable(); }

    let exit_code = match cli.command {
        Commands::Lex { file, verbose }    => handle_lex(file, verbose),
        Commands::Parse { file, format }   => handle_parse(file, format),
        Commands::Check { file }           => handle_check(file),
        Commands::Run { file, args }       => handle_run(file, args),
    };
    std::process::exit(exit_code);
}

fn handle_lex(file: PathBuf, verbose: bool) -> i32 {
    Logger::info(&format!("Lexing: {:?}", file));
    let source = match fs::read_to_string(&file) {
        Ok(s)  => s,
        Err(e) => { Logger::error(&format!("Failed to read file: {}", e)); return 1; }
    };
    match lexer::tokenize(&source) {
        Ok(tokens) => {
            if verbose {
                println!("\n{} tokens:", tokens.len());
                println!("{:-<80}", "");
                for (idx, token) in tokens.iter().enumerate() {
                    println!("{:4} | {:?}", idx, token);
                }
                println!("{:-<80}", "");
            } else {
                Logger::info(&format!("✅ Lexing successful: {} tokens", tokens.len()));
            }
            0
        }
        Err(em) => { Logger::error("❌ Lexing failed:"); em.report_all(); 1 }
    }
}

fn handle_parse(file: PathBuf, _format: String) -> i32 {
    Logger::info(&format!("Parsing: {:?}", file));
    let source = match fs::read_to_string(&file) {
        Ok(s)  => s,
        Err(e) => { Logger::error(&format!("Failed to read file: {}", e)); return 1; }
    };
    let tokens = match lexer::tokenize(&source) {
        Ok(t)   => t,
        Err(em) => { Logger::error("❌ Lexing failed:"); em.report_all(); return 1; }
    };
    let arena = ast::arena::AstArena::with_capacity(256 * 1024);
    match parser::parse(&arena, tokens, source) {
        Ok(program) => {
            Logger::info(&format!(
                "✅ Parsing successful: {} top-level items",
                program.items.len()
            ));
            0
        }
        Err(em) => { Logger::error("❌ Parsing failed:"); em.report_all(); 1 }
    }
}

fn handle_check(_file: PathBuf) -> i32 {
    Logger::error("Check command not yet implemented"); 1
}

fn handle_run(_file: PathBuf, _args: Vec<String>) -> i32 {
    Logger::error("Run command not yet implemented"); 1
                   }
