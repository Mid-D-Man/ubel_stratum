//! Ubel Stratum Compiler CLI
 
mod lexer;
mod error_management;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use error_management::Logger;

#[derive(Parser)]
#[command(name = "stratc")]
#[command(about = "Ubel Stratum Compiler - Quantum-Ready Multi-Tier Language", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// Quiet mode (no logs)
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenize a .strat file (show tokens)
    Lex {
        /// Input file path
        file: PathBuf,

        /// Show detailed output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Parse a .strat file (show AST)
    Parse {
        /// Input file path
        file: PathBuf,

        /// Output format: text, json, debug
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Check syntax and types
    Check {
        /// Input file path
        file: PathBuf,
    },

    /// Run a .strat file (interpreter)
    Run {
        /// Input file path
        file: PathBuf,

        /// Arguments to pass to program
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Configure logger
    if cli.quiet {
        Logger::disable();
    }

    let exit_code = match cli.command {
        Commands::Lex { file, verbose } => handle_lex(file, verbose),
        Commands::Parse { file, format } => handle_parse(file, format),
        Commands::Check { file } => handle_check(file),
        Commands::Run { file, args } => handle_run(file, args),
    };

    std::process::exit(exit_code);
}

fn handle_lex(file: PathBuf, verbose: bool) -> i32 {
    Logger::info(&format!("Lexing: {:?}", file));

    let source = match fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            Logger::error(&format!("Failed to read file: {}", e));
            return 1;
        }
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
        Err(error_manager) => {
            Logger::error("❌ Lexing failed:");
            error_manager.report_all();
            1
        }
    }
}

fn handle_parse(_file: PathBuf, _format: String) -> i32 {
    Logger::error("Parse command not yet implemented");
    1
}

fn handle_check(_file: PathBuf) -> i32 {
    Logger::error("Check command not yet implemented");
    1
}

fn handle_run(_file: PathBuf, _args: Vec<String>) -> i32 {
    Logger::error("Run command not yet implemented");
    1
}
