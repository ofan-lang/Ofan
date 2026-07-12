#![allow(dead_code)] // skeleton — remove once all pipeline stages produce real output

use clap::Parser;
use std::path::PathBuf;

mod ast;
mod codegen;
mod lexer;
mod parser;
mod typechecker;

#[derive(Parser)]
#[command(name = "ofan", about = "The Ofan language compiler")]
struct Args {
    /// Source file to compile (.ofn)
    source: PathBuf,
}

fn main() {
    let args = Args::parse();

    let source = match std::fs::read_to_string(&args.source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ofan: cannot read '{}': {e}", args.source.display());
            std::process::exit(1);
        }
    };

    let tokens = match lexer::Lexer::new(&source).lex() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ofan: lex error: {e}");
            std::process::exit(1);
        }
    };

    let ast = match parser::Parser::new(tokens).parse() {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("ofan: parse error: {e}");
            std::process::exit(1);
        }
    };

    match typechecker::infer(&ast) {
        Ok(result) => {
            for w in &result.deferred {
                eprintln!("ofan: warning: {w}");
            }
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("ofan: type error: {e}");
            }
            std::process::exit(1);
        }
    }

    eprintln!(
        "ofan: codegen not yet implemented\nsource: {}",
        args.source.display()
    );
    std::process::exit(1);
}
